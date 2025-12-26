
use clap::Parser;
use tracing::{info, warn, error, debug};
use indicatif::{ProgressBar, ProgressStyle, ProgressDrawTarget};

use std::path::Path;
use crate::{
    database::{db_loader::open_db, init, queries},
    dlsite::{assign_data_to_work_with_client, DataSelection},
    folders::{get_list_of_folders, get_list_of_unscanned_works, register_folders, types::{ManagedFolder, RJCode}},
    tagger::{cover_art, process_work_folder, types::TaggerConfig},
    vpn::WireGuardManager,
    config::{Config, VpnProvider},
};

mod errors;
mod tagger;
mod dlsite;
mod folders;
mod database;
mod tag_manager;
mod circle_manager;
mod vpn;
mod config;

#[derive(Parser, Debug)]
struct PrgmArgs {
    // ===== STEP 1: SCAN =====
    /// Directory to scan for audio works (Step 1)
    #[arg(long)]
    input: Option<String>,

    /// Specific RJCode to process (Step 1)
    #[arg(long)]
    rjcode: Option<String>,

    // ===== STEP 2: FETCH METADATA =====
    /// Collect metadata from DLSite (Step 2)
    #[arg(long)]
    collect: bool,

    /// Download cover images from database links to work folders (Step 2)
    #[arg(long)]
    image: bool,

    // ===== STEP 3: TAG & CONVERT =====
    /// Apply tags to audio files (Step 3)
    #[arg(long)]
    tag: bool,

    /// Alias for --tag
    #[arg(long)]
    apply: bool,

    /// Convert files to MP3 320kbps (Step 3)
    #[arg(long)]
    convert: bool,

    // ===== WORKFLOW =====
    /// Run all 3 steps for newly scanned works (scan -> fetch metadata -> tag)
    #[arg(long)]
    full: bool,

    // ===== OTHER =====
    /// Move tagged files to destination
    #[arg(long)]
    r#move: Option<String>,

    /// Interactive tag management
    #[arg(long)]
    manage_tags: bool,

    /// Interactive circle management
    #[arg(long)]
    manage_circles: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let args = PrgmArgs::parse();
    let db = open_db(None)?;
    init(&db)?;

    // Handle tag management (early exit if specified)
    if args.manage_tags {
        tag_manager::run_interactive_tag_manager(&db)?;
        return Ok(());
    }

    // Handle circle management (early exit if specified)
    if args.manage_circles {
        circle_manager::run_interactive_circle_manager(&db)?;
        return Ok(());
    }

    // Check if we need VPN (only for metadata fetching)
    let needs_vpn = args.collect || args.image || args.full;

    // Load configuration
    let app_config = Config::load()?;

    // ========== PRE-VPN PHASE: Local filesystem operations ==========
    // Do all local scanning BEFORE connecting VPN to avoid losing access to network shares

    if args.input.is_some() || args.rjcode.is_some() {
        step1_scan(&db, &args)?;
    }

    // Pre-scan for images: identify which covers are missing BEFORE VPN connects
    let works_needing_covers = if args.image || args.full {
        info!("Pre-scanning for missing covers...");
        identify_works_needing_covers(&db)?
    } else {
        Vec::new()
    };

    // ========== VPN PHASE: Connect if needed ==========
    let mut vpn_manager: Option<WireGuardManager> = None;
    let mut was_vpn_already_connected = false;

    if needs_vpn && app_config.vpn.enabled {
        match app_config.vpn.provider {
            VpnProvider::Wireguard => {
                if let Some(ref wg_config) = app_config.vpn.wireguard {
                    let mut manager = WireGuardManager::new(wg_config)?;

                    // Check if VPN is already connected
                    was_vpn_already_connected = manager.interface_exists().unwrap_or(false);

                    if was_vpn_already_connected {
                        info!("VPN already connected, keeping it active");
                    } else {
                        info!("VPN enabled: Connecting to WireGuard...");
                        manager.connect()?;
                        info!("VPN connected, waiting for network to stabilize...");
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }

                    vpn_manager = Some(manager);
                } else {
                    warn!("WireGuard VPN enabled but no configuration found");
                }
            }
            _ => {
                warn!("VPN provider {:?} not yet implemented", app_config.vpn.provider);
            }
        }
    }

    // Create HTTP client (now using system DNS resolver instead of hickory-dns)
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .cookie_store(true)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;

    // ========== WORKFLOW EXECUTION (with VPN active if needed) ==========
    let result = if args.full {
        // Full workflow: fetch metadata -> download images
        run_full_workflow(&db, &args, &http_client, &app_config, &works_needing_covers).await
    } else {
        // Individual steps (VPN-dependent operations only)
        if args.collect {
            step2_fetch_metadata(&db, &args, &http_client).await?;
        }

        if args.image && !works_needing_covers.is_empty() {
            step2_download_images_filtered(&db, &works_needing_covers).await?;
        }
        Ok(())
    };

    // Disconnect VPN before post-VPN operations
    if let Some(mut manager) = vpn_manager {
        if !was_vpn_already_connected {
            info!("Disconnecting VPN (was not connected initially)...");
            manager.disconnect()?;
        } else {
            info!("VPN was already connected initially, keeping it active");
        }
    }

    // Return early on error before post-VPN phase
    result?;

    // ========== POST-VPN PHASE: Local filesystem operations ==========
    // Copy cached covers and tag files AFTER VPN is disconnected to ensure network share access

    // Copy covers from cache to their final destinations
    if (args.image || args.full) && !works_needing_covers.is_empty() {
        step_copy_cached_covers(&works_needing_covers)?;
    }

    if args.tag || args.apply || args.convert || args.full {
        step3_tag_files(&db, &args, &app_config).await?;
    }

    // Move files if requested
    if let Some(ref destination) = args.r#move {
        step_move_files(&db, &args, destination)?;
    }

    Ok(())
}

/// Helper function to create a progress bar that keeps finished items visible
fn create_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_draw_target(ProgressDrawTarget::stdout());
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-")
    );
    pb
}

/// Step 1: Scan directories for audio works
fn step1_scan(db: &rusqlite::Connection, args: &PrgmArgs) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== STEP 1: SCANNING FOLDERS ===");

    let scan_path = if let Some(ref input) = args.input {
        input.clone()
    } else if let Some(ref rjcode) = args.rjcode {
        // If --rjcode is provided, scan current directory
        std::env::current_dir()?.to_string_lossy().to_string()
    } else {
        // Scan current directory by default
        std::env::current_dir()?.to_string_lossy().to_string()
    };

    info!("Scanning: {}", scan_path);

    let folders = get_list_of_folders(&scan_path)?;
    info!("Found {} valid RJ folders", folders.len());

    if !folders.is_empty() {
        register_folders(db, folders)?;
        info!("Folders registered in database");
    }

    Ok(())
}

/// Step 2: Fetch metadata from DLSite
async fn step2_fetch_metadata(
    db: &rusqlite::Connection,
    args: &PrgmArgs,
    http_client: &reqwest::Client,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== STEP 2: FETCHING METADATA FROM DLSITE ===");

    // Build data selection based on CLI args
    let data_selection = DataSelection {
        tags: args.collect,
        release_date: args.collect,
        circle: args.collect,
        rating: args.collect,
        cvs: args.collect,
        stars: args.collect,
        cover_link: args.collect,  // Always fetch cover links when collecting metadata
    };

    // Get works to process
    let works = if args.rjcode.is_some() {
        // Process only specified RJCode
        vec![RJCode::new(args.rjcode.as_ref().unwrap().clone())?]
    } else {
        // Process all unscanned works
        get_list_of_unscanned_works(db, None)?
    };

    info!("Processing {} work(s)", works.len());

    // Create progress bar
    let pb = create_progress_bar(works.len() as u64);

    for work in works {
        pb.set_message(format!("Fetching {}", work));

        let result_msg = match assign_data_to_work_with_client(db, work.clone(), data_selection.clone(), Some(http_client)).await {
            Ok(_) => {
                format!("{} ✓", work)
            }
            Err(errors::HvtError::RemovedWork(rjcode)) => {
                if let Err(e) = queries::insert_error(db, &rjcode, "removed work", Some("dlsite_removed")) {
                    warn!("Failed to log error for {}: {}", work, e);
                }
                format!("{} (removed)", work)
            }
            Err(e) => {
                error!("Error fetching metadata for {}: {}", work, e);
                if let Err(err) = queries::insert_error(db, &work, &e.to_string(), Some("fetch_error")) {
                    warn!("Failed to log error for {}: {}", work, err);
                }
                format!("{} ✗", work)
            }
        };

        pb.println(&result_msg);
        pb.inc(1);
    }

    pb.finish_and_clear();

    Ok(())
}

/// Pre-scan phase: Identify works that need covers BEFORE VPN connects
/// This allows checking local/network filesystems before they become unavailable
fn identify_works_needing_covers(
    db: &rusqlite::Connection,
) -> Result<Vec<(RJCode, String, String)>, Box<dyn std::error::Error>> {
    // Get all works with cover links
    let works_with_covers = queries::get_all_works_with_cover_links(db)?;

    if works_with_covers.is_empty() {
        info!("No works with cover links found in database");
        return Ok(Vec::new());
    }

    let mut works_needing_covers = Vec::new();

    for (work, folder_path, cover_url) in works_with_covers {
        let folder_path_obj = Path::new(&folder_path);

        // Skip if folder doesn't exist
        if !folder_path_obj.exists() {
            debug!("Folder not found: {} ({})", work, folder_path);
            continue;
        }

        // Skip if cover already exists
        if cover_art::has_cover_art(folder_path_obj) {
            debug!("Cover already exists: {} ({})", work, folder_path);
            continue;
        }

        // This work needs a cover
        works_needing_covers.push((work, folder_path, cover_url));
    }

    if !works_needing_covers.is_empty() {
        info!("Found {} work(s) needing covers", works_needing_covers.len());
    }

    Ok(works_needing_covers)
}

/// Step 2b: Download cover images to local cache (VPN phase)
async fn step2_download_images_filtered(
    db: &rusqlite::Connection,
    works_to_download: &[(RJCode, String, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== DOWNLOADING COVER IMAGES TO CACHE ===");

    if works_to_download.is_empty() {
        info!("No covers to download (all works already have covers)");
        return Ok(());
    }

    info!("Downloading {} cover(s) to local cache...", works_to_download.len());

    // Create progress bar
    let pb = create_progress_bar(works_to_download.len() as u64);

    let mut downloaded = 0;
    let mut failed = 0;

    for (work, _folder_path, cover_url) in works_to_download {
        pb.set_message(format!("Downloading {}", work));

        let result_msg = match cover_art::download_cover_to_cache(
            cover_url,
            work.as_str(),
            None,  // Keep original dimensions from DLSite
        ).await {
            Ok(_cache_path) => {
                downloaded += 1;
                format!("{} ✓", work)
            }
            Err(e) => {
                warn!("Failed to download cover for {}: {}", work, e);
                failed += 1;
                format!("{} ✗", work)
            }
        };

        pb.println(&result_msg);
        pb.inc(1);
    }

    pb.finish_and_clear();
    info!("Covers cached: {} | Failed: {}", downloaded, failed);

    Ok(())
}

/// Post-VPN: Copy cached covers to their final folder destinations
fn step_copy_cached_covers(
    works_with_covers: &[(RJCode, String, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    if works_with_covers.is_empty() {
        return Ok(());
    }

    info!("\n=== COPYING CACHED COVERS TO FOLDERS ===");
    info!("Copying {} cover(s) from cache...", works_with_covers.len());

    // Create progress bar
    let pb = create_progress_bar(works_with_covers.len() as u64);

    let mut copied = 0;
    let mut failed = 0;

    for (work, folder_path, _cover_url) in works_with_covers {
        pb.set_message(format!("Copying {}", work));
        let folder_path_obj = Path::new(folder_path);

        // Skip if folder doesn't exist
        if !folder_path_obj.exists() {
            debug!("Folder not found, skipping: {}", folder_path);
            pb.println(&format!("{} (folder not found)", work));
            failed += 1;
            pb.inc(1);
            continue;
        }

        let result_msg = match cover_art::copy_cover_from_cache(work.as_str(), folder_path_obj) {
            Ok(_) => {
                copied += 1;
                format!("{} ✓", work)
            }
            Err(e) => {
                warn!("Failed to copy cover for {}: {}", work, e);
                failed += 1;
                format!("{} ✗", work)
            }
        };

        pb.println(&result_msg);
        pb.inc(1);
    }

    pb.finish_and_clear();
    info!("Covers copied: {} | Failed: {}", copied, failed);

    Ok(())
}

/// Step 3: Tag and convert audio files
async fn step3_tag_files(
    db: &rusqlite::Connection,
    args: &PrgmArgs,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== STEP 3: TAGGING AUDIO FILES ===");

    // Create tagger config from CLI arguments and app config
    let tagger_config = TaggerConfig {
        convert_to_mp3: args.convert,
        target_bitrate: 320,
        download_cover: args.image,
        tag_separator: app_config.tagger.get_separator(),
    };

    // Get works to process with their paths
    let works_with_paths: Vec<(RJCode, String)> = if let Some(ref rjcode) = args.rjcode {
        // For specific RJCode, use current directory or input path
        let path = if let Some(ref input) = args.input {
            input.clone()
        } else {
            std::env::current_dir()?.to_string_lossy().to_string()
        };
        vec![(RJCode::new(rjcode.clone())?, path)]
    } else {
        // Get all works from DB with their stored paths
        queries::get_all_works_with_paths(db)?
    };

    info!("Processing {} work(s)", works_with_paths.len());

    // Create progress bar
    let pb = create_progress_bar(works_with_paths.len() as u64);

    for (work, folder_path) in works_with_paths {
        pb.set_message(format!("Tagging {}", work));

        if !std::path::Path::new(&folder_path).exists() {
            warn!("Folder not found: {}", folder_path);
            pb.println(&format!("{} (folder not found)", work));
            pb.inc(1);
            continue;
        }

        let folder = ManagedFolder::new(folder_path.clone());

        let result_msg = match process_work_folder(db, &folder, &tagger_config).await {
            Ok(_) => {
                format!("{} ✓", work)
            }
            Err(e) => {
                warn!("Failed to tag {}: {}", work, e);
                format!("{} ✗", work)
            }
        };

        pb.println(&result_msg);
        pb.inc(1);
    }

    pb.finish_and_clear();
    info!("Tagging completed");

    Ok(())
}

/// Move tagged files to destination directory
fn step_move_files(
    db: &rusqlite::Connection,
    args: &PrgmArgs,
    destination: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== MOVING FILES TO DESTINATION ===");
    info!("Destination: {}", destination);

    // Create destination directory if it doesn't exist
    let dest_path = Path::new(destination);
    if !dest_path.exists() {
        std::fs::create_dir_all(dest_path)?;
        info!("Created destination directory: {}", destination);
    }

    // Get works to move
    let works_with_paths: Vec<(RJCode, String)> = if let Some(ref input) = args.input {
        // If --input is specified, only move works from that directory
        let folders = get_list_of_folders(input)?;
        folders.into_iter()
            .map(|f| (f.rjcode.clone(), f.path.clone()))
            .collect()
    } else if let Some(ref rjcode) = args.rjcode {
        // If --rjcode is specified, move only that specific work
        let path = std::env::current_dir()?.to_string_lossy().to_string();
        vec![(RJCode::new(rjcode.clone())?, path)]
    } else {
        // Move all works from database
        queries::get_all_works_with_paths(db)?
    };

    if works_with_paths.is_empty() {
        info!("No works to move");
        return Ok(());
    }

    info!("Moving {} work(s)...", works_with_paths.len());

    let pb = create_progress_bar(works_with_paths.len() as u64);

    let mut moved = 0;
    let mut failed = 0;

    for (work, old_path) in works_with_paths {
        pb.set_message(format!("Moving {}", work));

        let old_path_obj = Path::new(&old_path);

        if !old_path_obj.exists() {
            warn!("Source folder not found: {}", old_path);
            pb.println(&format!("{} (source not found)", work));
            failed += 1;
            pb.inc(1);
            continue;
        }

        // New path: destination/folder_name
        let folder_name = old_path_obj.file_name()
            .ok_or_else(|| format!("Invalid folder path: {}", old_path))?;
        let new_path = dest_path.join(folder_name);

        // Move the folder
        match std::fs::rename(old_path_obj, &new_path) {
            Ok(_) => {
                // Update path in database
                let new_path_str = new_path.to_string_lossy().to_string();
                match queries::update_folder_path(db, &work, &new_path_str) {
                    Ok(_) => {
                        pb.println(&format!("{} ✓", work));
                        moved += 1;
                    }
                    Err(e) => {
                        warn!("Moved folder but failed to update DB for {}: {}", work, e);
                        pb.println(&format!("{} ⚠ (DB update failed)", work));
                        failed += 1;
                    }
                }
            }
            Err(e) => {
                warn!("Failed to move {}: {}", work, e);
                pb.println(&format!("{} ✗", work));
                failed += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();
    info!("Moved: {} | Failed: {}", moved, failed);

    Ok(())
}

/// Full workflow: fetch metadata -> download covers
/// Note: Scanning and tagging are done outside this function to manage VPN connection properly
async fn run_full_workflow(
    db: &rusqlite::Connection,
    args: &PrgmArgs,
    http_client: &reqwest::Client,
    app_config: &Config,
    works_needing_covers: &[(RJCode, String, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== RUNNING FULL WORKFLOW (VPN PHASE) ===\n");

    // Step 2: Fetch metadata for newly scanned works
    let unscanned_works = get_list_of_unscanned_works(db, None)?;

    if unscanned_works.is_empty() {
        info!("No new works to process");
        return Ok(());
    }

    info!("Found {} newly scanned work(s)", unscanned_works.len());

    let data_selection = DataSelection {
        tags: true,
        release_date: true,
        circle: true,
        rating: true,
        cvs: true,
        stars: true,
        cover_link: true,
    };

    // Create progress bar
    let pb = create_progress_bar(unscanned_works.len() as u64);

    for work in &unscanned_works {
        pb.set_message(format!("Fetching {}", work));

        let result_msg = match assign_data_to_work_with_client(db, work.clone(), data_selection.clone(), Some(http_client)).await {
            Ok(_) => {
                format!("{} ✓", work)
            }
            Err(errors::HvtError::RemovedWork(rjcode)) => {
                queries::insert_error(db, &rjcode, "removed work", Some("dlsite_removed"))?;
                format!("{} (removed)", work)
            }
            Err(e) => {
                error!("Error processing {}: {}", work, e);
                queries::insert_error(db, work, &e.to_string(), Some("fetch_error"))?;
                format!("{} ✗", work)
            }
        };

        pb.println(&result_msg);
        pb.inc(1);
    }

    pb.finish_and_clear();
    info!("Metadata fetch completed");

    // Step 3: Download covers (using pre-filtered list from pre-VPN phase)
    if !works_needing_covers.is_empty() {
        step2_download_images_filtered(db, works_needing_covers).await?;
    }

    info!("\n=== VPN PHASE COMPLETED ===");
    info!("Tagging will be performed after VPN disconnects...");
    Ok(())
}
