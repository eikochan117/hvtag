
use clap::Parser;
use tracing::{info, warn, error, debug};

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

    // Determine workflow mode
    let result = if args.full {
        // Full workflow: scan -> fetch metadata -> tag
        run_full_workflow(&db, &args, &http_client, &app_config).await
    } else {
        // Individual steps
        if args.input.is_some() || args.rjcode.is_some() {
            step1_scan(&db, &args)?;
        }

        if args.collect {
            step2_fetch_metadata(&db, &args, &http_client).await?;
        }

        if args.image {
            step2_download_images(&db).await?;
        }

        if args.tag || args.apply || args.convert {
            step3_tag_files(&db, &args, &app_config).await?;
        }
        Ok(())
    };

    // Disconnect VPN before exiting (even on error)
    // Only disconnect if we connected it ourselves (it wasn't already connected)
    if let Some(mut manager) = vpn_manager {
        if !was_vpn_already_connected {
            info!("Disconnecting VPN (was not connected initially)...");
            manager.disconnect()?;
        } else {
            info!("VPN was already connected initially, keeping it active");
        }
    }

    result
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

    for work in works {
        info!("Fetching metadata for {}...", work);

        match assign_data_to_work_with_client(db, work.clone(), data_selection.clone(), Some(http_client)).await {
            Ok(_) => info!("{} ... metadata fetched successfully", work),
            Err(errors::HvtError::RemovedWork(rjcode)) => {
                warn!("{} ... is removed from DLSite!", work);
                if let Err(e) = queries::insert_error(db, &rjcode, "removed work", Some("dlsite_removed")) {
                    warn!("Failed to log error for {}: {}", work, e);
                }
            }
            Err(e) => {
                error!("Error fetching metadata for {}: {}", work, e);
                if let Err(err) = queries::insert_error(db, &work, &e.to_string(), Some("fetch_error")) {
                    warn!("Failed to log error for {}: {}", work, err);
                }
            }
        }
    }

    Ok(())
}

/// Step 2b: Download cover images from database links to work folders
async fn step2_download_images(
    db: &rusqlite::Connection,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== DOWNLOADING COVER IMAGES ===");

    // Get all works with cover links
    let works_with_covers = queries::get_all_works_with_cover_links(db)?;

    if works_with_covers.is_empty() {
        info!("No works with cover links found in database");
        info!("Run --collect --image first to fetch cover links from DLSite");
        return Ok(());
    }

    info!("Found {} work(s) with cover links", works_with_covers.len());

    let mut downloaded = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for (work, folder_path, cover_url) in works_with_covers {
        let folder_path_obj = Path::new(&folder_path);

        // Skip if folder doesn't exist
        if !folder_path_obj.exists() {
            warn!("Folder not found: {} ({})", work, folder_path);
            failed += 1;
            continue;
        }

        // Skip if cover already exists
        if cover_art::has_cover_art(folder_path_obj) {
            debug!("Cover already exists: {} ({})", work, folder_path);
            skipped += 1;
            continue;
        }

        info!("Downloading cover for {}...", work);
        match cover_art::download_and_save_cover(
            &cover_url,
            folder_path_obj,
            None,  // Keep original dimensions from DLSite
        ).await {
            Ok(_) => {
                info!("  {} ... cover downloaded", work);
                downloaded += 1;
            }
            Err(e) => {
                warn!("  {} ... failed to download cover: {}", work, e);
                failed += 1;
            }
        }
    }

    info!("\n=== COVER DOWNLOAD SUMMARY ===");
    info!("Downloaded: {}", downloaded);
    info!("Skipped (already exist): {}", skipped);
    info!("Failed: {}", failed);

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

    for (work, folder_path) in works_with_paths {
        if !std::path::Path::new(&folder_path).exists() {
            warn!("Folder not found: {}", folder_path);
            continue;
        }

        let folder = ManagedFolder::new(folder_path.clone());
        info!("Tagging files in {}...", folder_path);

        match process_work_folder(db, &folder, &tagger_config).await {
            Ok(_) => info!("{} ... tagged successfully", work),
            Err(e) => warn!("Failed to tag {}: {}", work, e),
        }
    }

    Ok(())
}

/// Full workflow: scan -> fetch metadata -> tag
async fn run_full_workflow(
    db: &rusqlite::Connection,
    args: &PrgmArgs,
    http_client: &reqwest::Client,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== RUNNING FULL WORKFLOW ===\n");

    // Step 1: Scan
    step1_scan(db, args)?;

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

    for work in &unscanned_works {
        info!("Fetching metadata for {}...", work);
        match assign_data_to_work_with_client(db, work.clone(), data_selection.clone(), Some(http_client)).await {
            Ok(_) => info!("{} ... metadata fetched", work),
            Err(errors::HvtError::RemovedWork(rjcode)) => {
                warn!("{} ... is removed from DLSite!", work);
                queries::insert_error(db, &rjcode, "removed work", Some("dlsite_removed"))?;
            }
            Err(e) => {
                error!("Error processing {}: {}", work, e);
                queries::insert_error(db, work, &e.to_string(), Some("fetch_error"))?;
            }
        }
    }

    // Step 3: Tag files
    let tagger_config = TaggerConfig {
        convert_to_mp3: args.convert,
        target_bitrate: 320,
        download_cover: true,  // Always download covers in full mode
        tag_separator: app_config.tagger.get_separator(),
    };

    // Get the newly scanned works with their paths from DB
    let works_with_paths = queries::get_unscanned_works_with_paths(db)?;

    for (work, folder_path) in works_with_paths {
        if !std::path::Path::new(&folder_path).exists() {
            warn!("Folder not found: {}", folder_path);
            continue;
        }

        let folder = ManagedFolder::new(folder_path.clone());
        info!("Tagging files in {}...", folder_path);

        match process_work_folder(db, &folder, &tagger_config).await {
            Ok(_) => info!("{} ... tagged successfully", work),
            Err(e) => warn!("Failed to tag {}: {}", work, e),
        }
    }

    info!("\n=== FULL WORKFLOW COMPLETED ===");
    Ok(())
}
