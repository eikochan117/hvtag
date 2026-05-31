
use clap::Parser;
use tracing::{info, warn, error, debug};
use indicatif::{ProgressBar, ProgressStyle, ProgressDrawTarget};

use std::path::Path;
use crate::{
    database::{db_loader::open_db, init, queries},
    dlsite::{assign_data_to_work_with_client, DataSelection},
    folders::{get_list_of_folders, get_list_of_unscanned_works, register_folders, types::{ManagedFolder, RJCode}},
    tagger::{cover_art, converter, folder_normalizer, process_work_folder, types::TaggerConfig},
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
    // ===== IMPORT WORKFLOW =====
    /// Import: scan from config source_path, process, then move to library_path
    #[arg(long)]
    import: bool,

    /// Scan library_path and add missing works to database
    #[arg(long)]
    scan: bool,

    /// Specific work code to process (RJxxxxxx or VJxxxxxx)
    #[arg(long)]
    rjcode: Option<String>,

    // ===== PROCESSING STEPS =====
    /// Collect metadata from DLSite
    #[arg(long)]
    collect: bool,

    /// Download cover images from database links to work folders
    #[arg(long)]
    image: bool,

    /// Apply tags to audio files
    #[arg(long)]
    tag: bool,

    /// Alias for --tag
    #[arg(long)]
    apply: bool,

    /// Convert files to MP3 320kbps
    #[arg(long)]
    convert: bool,

    /// Force re-tag all files (ignore already tagged status)
    #[arg(long)]
    force: bool,

    // ===== COMBINED WORKFLOWS =====
    /// Full pipeline: import + collect + image + tag (equivalent to --import --collect --image --tag)
    #[arg(long)]
    full: bool,

    // ===== OTHER =====
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

    let mut args = PrgmArgs::parse();
    let db = open_db(None)?;
    init(&db)?;

    // --full is a shortcut for --import --collect --image --tag
    if args.full {
        args.import = true;
        args.collect = true;
        args.image = true;
        args.tag = true;
    }

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

    // Load configuration
    let app_config = Config::load()?;

    // --scan: Add existing works from library_path to database
    if args.scan {
        run_scan_workflow(&db, &app_config)?;
        return Ok(());
    }

    // --import workflow (for new works from source directory)
    if args.import {
        run_import_workflow(&db, &args, &app_config).await?;
        return Ok(());
    }

    // Standalone commands for existing works in database
    let has_action = args.collect || args.image || args.tag || args.apply || args.convert;

    if !has_action {
        info!("No action specified. Use --import to process new works, or --help for options.");
        info!("Use --collect/--tag/--image to process existing works in database.");
        return Ok(());
    }

    // Run standalone workflow for existing database works
    run_standalone_workflow(&db, &args, &app_config).await?;

    Ok(())
}

/// Scan workflow: Add existing works from library_path to database
fn run_scan_workflow(
    db: &rusqlite::Connection,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let library_path = app_config.import.library_path.as_ref()
        .ok_or_else(|| errors::HvtError::Generic(
            "Please configure import.library_path in config.toml".to_string()
        ))?;

    info!("=== SCAN WORKFLOW ===");
    info!("Library: {}", library_path);

    // Scan library directory for RJ folders
    info!("\n--- Scanning library directory ---");
    let library_folders = get_list_of_folders(library_path)?;

    if library_folders.is_empty() {
        info!("No valid RJ folders found in library directory");
        return Ok(());
    }

    info!("Found {} folder(s) in library", library_folders.len());

    // Get existing works from database
    let existing_works: std::collections::HashSet<String> = queries::get_all_works(db)?
        .into_iter()
        .map(|rj| rj.to_string())
        .collect();

    // Filter to only new works
    let new_folders: Vec<_> = library_folders
        .into_iter()
        .filter(|f| !existing_works.contains(&f.rjcode.to_string()))
        .collect();

    if new_folders.is_empty() {
        info!("All folders are already registered in database");
        return Ok(());
    }

    info!("Found {} new folder(s) to register", new_folders.len());

    // Register new folders
    let pb = create_progress_bar(new_folders.len() as u64);
    let mut registered = 0;

    for folder in &new_folders {
        pb.set_message(format!("Registering {}", folder.rjcode));

        match register_folders(db, vec![folder.clone()]) {
            Ok(_) => {
                pb.println(&format!("{} ✓", folder.rjcode));
                registered += 1;
            }
            Err(e) => {
                warn!("Failed to register {}: {}", folder.rjcode, e);
                pb.println(&format!("{} ✗", folder.rjcode));
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();
    info!("\n=== SCAN COMPLETE ===");
    info!("Registered: {} new work(s)", registered);

    Ok(())
}

/// Standalone workflow for existing works in database
async fn run_standalone_workflow(
    db: &rusqlite::Connection,
    args: &PrgmArgs,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== STANDALONE WORKFLOW (existing works) ===");

    // Identify works that need cover downloads
    let works_needing_covers = if args.image {
        identify_works_needing_covers(db)?
    } else {
        vec![]
    };

    // Get all works with cover links from database (for tagging)
    let all_works_with_covers = queries::get_all_works_with_cover_links(db)?;

    // Setup VPN if needed for network operations
    let needs_vpn = args.collect || (args.image && !works_needing_covers.is_empty());
    let mut vpn_manager: Option<vpn::wireguard::WireGuardManager> = None;

    debug!("VPN check: needs_vpn={}, vpn.enabled={}, wireguard={:?}",
           needs_vpn, app_config.vpn.enabled, app_config.vpn.wireguard.is_some());

    if needs_vpn && app_config.vpn.enabled {
        if let Some(ref wg_config) = app_config.vpn.wireguard {
            info!("Connecting VPN...");
            let mut manager = vpn::wireguard::WireGuardManager::new(wg_config)?;
            manager.connect()?;
            vpn_manager = Some(manager);
        } else {
            warn!("VPN enabled but no wireguard config found!");
        }
    } else if needs_vpn {
        info!("VPN needed but disabled in config, skipping...");
    }

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // --collect: Fetch metadata for works in database
    if args.collect {
        step2_fetch_metadata(db, args, &http_client).await?;
    }

    // --image: Download covers for works in database
    if args.image && !works_needing_covers.is_empty() {
        step2_download_images_filtered(db, &works_needing_covers).await?;
    }

    // Disconnect VPN after network operations
    if let Some(ref mut manager) = vpn_manager {
        info!("Disconnecting VPN...");
        manager.disconnect()?;
    }

    // --convert (standalone): Convert non-MP3 files to MP3 without tagging
    // When combined with --tag, this runs first so the tag pipeline finds only MP3s
    if args.convert {
        step_convert_files(db).await?;
    }

    // --tag / --apply: Tag files for works in database
    if args.tag || args.apply {
        // First copy cached covers to folders
        step_copy_cached_covers(&all_works_with_covers)?;
        // Then tag (convert_to_mp3 in TaggerConfig will be a no-op if --convert already ran)
        step3_tag_files(db, args, app_config).await?;
    }

    info!("=== DONE ===");
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

/// Convert non-MP3 audio files (FLAC, WAV, OGG) to MP3 for all works in database
async fn step_convert_files(
    db: &rusqlite::Connection,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== CONVERTING AUDIO FILES TO MP3 ===");

    if !converter::is_ffmpeg_available() {
        error!("FFmpeg not found in PATH. Please install FFmpeg to use --convert.");
        return Err(Box::new(errors::HvtError::AudioConversion(
            "FFmpeg not found in PATH. Please install FFmpeg: https://ffmpeg.org/".to_string(),
        )));
    }

    let target_bitrate: u32 = 320;
    let non_mp3_ext = ["flac", "wav", "ogg"];

    let works_with_paths = queries::get_all_works_with_paths(db)?;

    if works_with_paths.is_empty() {
        info!("No works in database");
        return Ok(());
    }

    // Pre-scan: collect only works that have non-MP3 files
    let mut works_to_convert: Vec<(RJCode, Vec<std::path::PathBuf>)> = Vec::new();

    for (rjcode, folder_path) in &works_with_paths {
        let folder = std::path::Path::new(folder_path);
        if !folder.exists() {
            continue;
        }
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if non_mp3_ext.contains(&ext.as_str()) {
                    files.push(path);
                }
            }
        }
        if !files.is_empty() {
            works_to_convert.push((rjcode.clone(), files));
        }
    }

    if works_to_convert.is_empty() {
        info!("No non-MP3 files found, nothing to convert");
        return Ok(());
    }

    let total_files: usize = works_to_convert.iter().map(|(_, f)| f.len()).sum();
    info!(
        "Found {} work(s) with {} file(s) to convert",
        works_to_convert.len(),
        total_files
    );

    let pb = create_progress_bar(total_files as u64);
    let mut converted = 0usize;
    let mut failed = 0usize;

    for (rjcode, files) in &works_to_convert {
        for file_path in files {
            let filename = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            pb.set_message(format!("{} — {}", rjcode, filename));

            match converter::convert_to_mp3_in_place(file_path, target_bitrate).await {
                Ok(_) => {
                    pb.println(format!("{}/{} ✓", rjcode, filename));
                    converted += 1;
                }
                Err(e) => {
                    warn!("Failed to convert {}/{}: {}", rjcode, filename, e);
                    pb.println(format!("{}/{} ✗", rjcode, filename));
                    failed += 1;
                }
            }

            pb.inc(1);
        }
    }

    pb.finish_and_clear();
    info!("Converted: {} | Failed: {}", converted, failed);

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
        force_retag: args.force,
    };

    // Get works to process with their paths
    let works_with_paths: Vec<(RJCode, String)> = if let Some(ref rjcode) = args.rjcode {
        // For specific RJCode, use current directory
        let path = std::env::current_dir()?.to_string_lossy().to_string();
        vec![(RJCode::new(rjcode.clone())?, path)]
    } else {
        // Pre-filter: only works that actually need tagging, so the progress bar
        // reflects real work to do rather than the full DB size.
        queries::get_all_works_with_paths(db)?
            .into_iter()
            .filter(|(rjcode, path)| {
                if tagger_config.force_retag {
                    return true;
                }
                let folder = ManagedFolder::new(path.clone());
                if !folder.is_tagged {
                    return true;
                }
                let needs_retag_tags =
                    crate::database::custom_tags::should_retag_work(db, rjcode)
                        .unwrap_or(false);
                let needs_retag_circle =
                    crate::database::custom_circles::should_retag_work_for_circle(db, rjcode)
                        .unwrap_or(false);
                needs_retag_tags || needs_retag_circle
            })
            .collect()
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

/// Move folder with cross-drive support (copy + delete fallback)
fn move_folder_cross_drive(source: &Path, target: &Path) -> Result<(), errors::HvtError> {
    // Try rename first (fast, works on same drive)
    match std::fs::rename(source, target) {
        Ok(_) => Ok(()),
        Err(e) => {
            // Check if it's a cross-device error (errno 17 on Unix, various on Windows)
            let is_cross_device = e.raw_os_error().map_or(false, |code| {
                // EXDEV on Unix, ERROR_NOT_SAME_DEVICE on Windows
                code == 17 || code == 18 || code == 0x11
            });

            if is_cross_device || cfg!(target_os = "windows") {
                // Fallback: copy then delete
                debug!("Cross-drive move detected, using copy+delete for {}", source.display());
                copy_dir_recursive(source, target)?;
                std::fs::remove_dir_all(source)
                    .map_err(|e| errors::HvtError::Generic(format!(
                        "Failed to remove source after copy: {}", e
                    )))?;
                Ok(())
            } else {
                Err(errors::HvtError::Generic(format!("Failed to move folder: {}", e)))
            }
        }
    }
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), errors::HvtError> {
    std::fs::create_dir_all(dst)
        .map_err(|e| errors::HvtError::Generic(format!("Failed to create directory {}: {}", dst.display(), e)))?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| errors::HvtError::Generic(format!("Failed to read directory {}: {}", src.display(), e)))?
    {
        let entry = entry.map_err(|e| errors::HvtError::Generic(format!("Failed to read entry: {}", e)))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| errors::HvtError::Generic(format!(
                    "Failed to copy {} to {}: {}", src_path.display(), dst_path.display(), e
                )))?;
        }
    }

    Ok(())
}

/// Import workflow: scan source -> process -> move to library
async fn run_import_workflow(
    db: &rusqlite::Connection,
    args: &PrgmArgs,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // Validate config
    let source_path = app_config.import.source_path.as_ref()
        .ok_or_else(|| errors::HvtError::Generic(
            "Please configure import.source_path in config.toml".to_string()
        ))?;
    let library_path = app_config.import.library_path.as_ref()
        .ok_or_else(|| errors::HvtError::Generic(
            "Please configure import.library_path in config.toml".to_string()
        ))?;

    info!("=== IMPORT WORKFLOW ===");
    info!("Source: {}", source_path);
    info!("Library: {}", library_path);

    // ========== PRE-VPN PHASE ==========
    // 1. Prepare source folders: rename non-RJ roots and flatten audio files
    info!("\n--- Preparing source folders ---");
    match folder_normalizer::prepare_source_directory(source_path) {
        Ok(0) => debug!("All source folders already normalized"),
        Ok(n) => info!("Prepared {} folder(s)", n),
        Err(e) => warn!("Folder preparation encountered an error: {}", e),
    }

    // 2. Scan source directory
    info!("\n--- Scanning source directory ---");
    let source_folders = get_list_of_folders(source_path)?;

    if source_folders.is_empty() {
        info!("No valid RJ folders found in source directory");
        return Ok(());
    }

    info!("Found {} folder(s) to import", source_folders.len());

    // 2. Filter out folders that already exist in library
    let library_path_obj = Path::new(library_path);
    if !library_path_obj.exists() {
        std::fs::create_dir_all(library_path_obj)?;
        info!("Created library directory: {}", library_path);
    }

    let mut folders_to_process: Vec<ManagedFolder> = Vec::new();
    for folder in source_folders {
        let folder_name = Path::new(&folder.path).file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let target_path = library_path_obj.join(folder_name);

        if target_path.exists() {
            warn!("{} already exists in library, skipping", folder.rjcode);
        } else {
            folders_to_process.push(folder);
        }
    }

    if folders_to_process.is_empty() {
        info!("All folders already exist in library, nothing to import");
        return Ok(());
    }

    info!("{} folder(s) to process", folders_to_process.len());

    // Register folders in DB now (with source path) so that --collect and --tag can resolve
    // fld_id during this same run. The path will be updated to the library path after the move.
    info!("\n--- Registering folders in database ---");
    for folder in &folders_to_process {
        if let Err(e) = register_folders(db, vec![folder.clone()]) {
            warn!("Failed to register {} in DB: {}", folder.rjcode, e);
        }
    }

    // Check if we need VPN
    let needs_vpn = args.collect || args.image;

    // ========== VPN PHASE ==========
    let mut vpn_manager: Option<WireGuardManager> = None;

    if needs_vpn && app_config.vpn.enabled {
        match app_config.vpn.provider {
            VpnProvider::Wireguard => {
                if let Some(ref wg_config) = app_config.vpn.wireguard {
                    let mut manager = WireGuardManager::new(wg_config)?;

                    if manager.interface_exists().unwrap_or(false) {
                        info!("VPN already connected, reusing");
                    } else {
                        info!("Connecting VPN...");
                        manager.connect()?;
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }

                    vpn_manager = Some(manager);
                }
            }
            _ => warn!("VPN provider {:?} not implemented", app_config.vpn.provider),
        }
    }

    // Create HTTP client
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .cookie_store(true)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;

    // Collect metadata if requested
    if args.collect {
        info!("\n--- Fetching metadata ---");
        let data_selection = DataSelection {
            tags: true,
            release_date: true,
            circle: true,
            rating: true,
            cvs: true,
            stars: true,
            cover_link: true,
        };

        let pb = create_progress_bar(folders_to_process.len() as u64);

        for folder in &folders_to_process {
            pb.set_message(format!("Fetching {}", folder.rjcode));

            let result_msg = match assign_data_to_work_with_client(
                db, folder.rjcode.clone(), data_selection.clone(), Some(&http_client)
            ).await {
                Ok(_) => format!("{} ✓", folder.rjcode),
                Err(errors::HvtError::RemovedWork(rjcode)) => {
                    queries::insert_error(db, &rjcode, "removed work", Some("dlsite_removed"))?;
                    format!("{} (removed)", folder.rjcode)
                }
                Err(e) => {
                    error!("Error fetching {}: {}", folder.rjcode, e);
                    format!("{} ✗", folder.rjcode)
                }
            };

            pb.println(&result_msg);
            pb.inc(1);
        }

        pb.finish_and_clear();
    }

    // Download covers if requested
    if args.image {
        info!("\n--- Downloading covers ---");

        // Filter folders that need covers (don't have folder.jpeg yet)
        let folders_needing_covers: Vec<_> = folders_to_process.iter()
            .filter(|f| !cover_art::has_cover_art(Path::new(&f.path)))
            .collect();

        if folders_needing_covers.is_empty() {
            info!("All folders already have covers, skipping download");
        } else {
            info!("{} folder(s) need covers", folders_needing_covers.len());
            let pb = create_progress_bar(folders_needing_covers.len() as u64);

            for folder in &folders_needing_covers {
                pb.set_message(format!("Cover {}", folder.rjcode));

                // Get cover URL from database
                if let Ok(Some(cover_url)) = queries::get_cover_link(db, &folder.rjcode) {
                    match cover_art::download_cover_to_cache(&cover_url, &folder.rjcode.to_string(), Some((500, 500))).await {
                        Ok(_) => pb.println(&format!("{} cover ✓", folder.rjcode)),
                        Err(e) => {
                            warn!("Failed to download cover for {}: {}", folder.rjcode, e);
                            pb.println(&format!("{} cover ✗", folder.rjcode));
                        }
                    }
                }

                pb.inc(1);
            }

            pb.finish_and_clear();
        }
    }

    // Disconnect VPN before filesystem operations
    drop(vpn_manager);

    // ========== POST-VPN PHASE ==========

    // Copy covers from cache to source folders (only for folders that don't have covers)
    if args.image {
        info!("\n--- Copying covers to folders ---");
        for folder in &folders_to_process {
            let folder_path = Path::new(&folder.path);

            // Skip if folder already has a cover
            if cover_art::has_cover_art(folder_path) {
                debug!("Skipping {}: already has cover", folder.rjcode);
                continue;
            }

            if let Err(e) = cover_art::copy_cover_from_cache(&folder.rjcode.to_string(), folder_path) {
                debug!("No cached cover for {}: {}", folder.rjcode, e);
            }
        }
    }

    // Tag files if requested
    if args.tag || args.apply {
        info!("\n--- Tagging files ---");
        let tagger_config = TaggerConfig {
            tag_separator: app_config.tagger.get_separator(),
            convert_to_mp3: args.convert,
            target_bitrate: 320,
            download_cover: args.image,
            force_retag: args.force,
        };

        let pb = create_progress_bar(folders_to_process.len() as u64);

        for folder in &folders_to_process {
            pb.set_message(format!("Tagging {}", folder.rjcode));

            let result_msg = match process_work_folder(db, folder, &tagger_config).await {
                Ok(_) => format!("{} tagged ✓", folder.rjcode),
                Err(e) => {
                    warn!("Failed to tag {}: {}", folder.rjcode, e);
                    format!("{} tag ✗", folder.rjcode)
                }
            };

            pb.println(&result_msg);
            pb.inc(1);
        }

        pb.finish_and_clear();
    }

    // Move folders to library and register in database
    info!("\n--- Moving to library ---");
    let pb = create_progress_bar(folders_to_process.len() as u64);
    let mut success_count = 0;
    let mut fail_count = 0;

    for folder in &folders_to_process {
        pb.set_message(format!("Moving {}", folder.rjcode));

        let source = Path::new(&folder.path);
        let folder_name = source.file_name()
            .ok_or_else(|| format!("Invalid path: {}", folder.path))?;
        let target = library_path_obj.join(folder_name);

        match move_folder_cross_drive(source, &target) {
            Ok(_) => {
                // Update path to final library location (folder was already registered earlier)
                let target_path_str = target.to_string_lossy().to_string();
                if let Err(e) = queries::update_folder_path(db, &folder.rjcode, &target_path_str) {
                    warn!("Moved {} but failed to update path in DB: {}", folder.rjcode, e);
                    pb.println(&format!("{} ⚠ (DB path error)", folder.rjcode));
                    fail_count += 1;
                } else {
                    pb.println(&format!("{} ✓", folder.rjcode));
                    success_count += 1;
                }
            }
            Err(e) => {
                warn!("Failed to move {}: {}", folder.rjcode, e);
                pb.println(&format!("{} ✗", folder.rjcode));
                fail_count += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();

    info!("\n=== IMPORT COMPLETE ===");
    info!("Imported: {} | Failed: {}", success_count, fail_count);

    Ok(())
}
