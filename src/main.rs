
use clap::Parser;
use tracing::{info, warn, error, debug};
use indicatif::{ProgressBar, ProgressStyle, ProgressDrawTarget};

use std::path::Path;
use crate::{
    database::{db_loader::open_db, init, queries},
    dlsite::{assign_data_to_work_with_client, DataSelection},
    folders::{get_list_of_folders, register_folders, types::{ManagedFolder, RJCode}},
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
mod web;

#[derive(Parser, Debug)]
struct PrgmArgs {
    /// Full pipeline: detect/format import folder, collect metadata+cover, tag files, move to library
    #[arg(long)]
    full: bool,

    /// Refresh an existing work already in the library (re-collect metadata/CVs/cover, re-tag files)
    #[arg(long)]
    retag: Option<String>,

    /// Refresh EVERY work already registered in the library (same as --retag, looped over all of them)
    #[arg(long)]
    full_retag: bool,

    /// One-shot test: run the full process on a folder in the import directory,
    /// without moving it or touching the database
    #[arg(long)]
    tag: Option<String>,

    /// Interactive tag management
    #[arg(long)]
    manage_tags: bool,

    /// Interactive circle management
    #[arg(long)]
    manage_circles: bool,

    /// Launch local web UI server (browse/search library, edit tag & circle mappings)
    #[arg(long)]
    ui: bool,

    /// Override the [ui] bind address/port from config.toml for this run.
    /// Accepts a bare host (keeps the configured port) or a full "host:port" (e.g. "0.0.0.0:8787").
    #[arg(long)]
    ui_bind: Option<String>,
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

    // Load configuration
    let app_config = Config::load()?;

    // --ui: Launch local web UI server (exclusive; needs config for bind address/port)
    if args.ui {
        web::run_ui_workflow(db, &app_config, args.ui_bind).await?;
        return Ok(());
    }

    // --retag <rjcode>: refresh an existing work already registered in the library
    if let Some(rjcode) = args.retag {
        run_retag_workflow(&db, &rjcode, &app_config).await?;
        return Ok(());
    }

    // --full-retag: refresh every work registered in the library
    if args.full_retag {
        run_full_retag_workflow(&db, &app_config).await?;
        return Ok(());
    }

    // --tag <folder>: one-shot test-tag a folder from the import directory, no DB/move
    if let Some(folder_name) = args.tag {
        run_tag_test_workflow(&db, &folder_name, &app_config).await?;
        return Ok(());
    }

    // --full: import workflow (new works from source directory)
    if args.full {
        run_import_workflow(&db, &app_config).await?;
        return Ok(());
    }

    info!("No action specified. Use --full to import new works, --retag <rjcode> to refresh an existing work, --tag <folder> to test-tag a folder without importing it, or --ui to browse the library.");
    Ok(())
}

/// Connects the configured VPN if enabled, reusing an already-active tunnel if present.
/// Used by `--retag`/`--tag`, which each need one DLSite fetch surrounded by connect/disconnect.
fn connect_vpn_if_enabled(app_config: &Config) -> Result<Option<WireGuardManager>, Box<dyn std::error::Error>> {
    if !app_config.vpn.enabled {
        return Ok(None);
    }
    let Some(ref wg_config) = app_config.vpn.wireguard else {
        warn!("VPN enabled but no wireguard config found!");
        return Ok(None);
    };

    let mut manager = WireGuardManager::new(wg_config)?;
    if manager.interface_exists().unwrap_or(false) {
        info!("VPN already connected, reusing");
    } else {
        info!("Connecting VPN...");
        manager.connect()?;
    }
    Ok(Some(manager))
}

/// Disconnects a VPN manager previously returned by `connect_vpn_if_enabled`, if any.
fn disconnect_vpn(manager: Option<WireGuardManager>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(mut m) = manager {
        info!("Disconnecting VPN...");
        m.disconnect()?;
    }
    Ok(())
}

/// Phase 1 of a refresh (needs VPN/DLSite access): re-collects tags/CVs/circle/rating/
/// release_date and caches a fresh cover to `~/.hvtag/covers_cache/`. Only the database and the
/// cover cache are touched here — no changes to the actual work folder — so this is safe to run
/// entirely while the VPN is up, mirroring `--full`'s pre-VPN-disconnect collect phase.
async fn refresh_metadata_and_cache_cover(
    db: &rusqlite::Connection,
    rjcode: &RJCode,
    http_client: &reqwest::Client,
) -> Result<(), Box<dyn std::error::Error>> {
    let data_selection = DataSelection {
        tags: true,
        release_date: true,
        circle: true,
        rating: true,
        cvs: true,
        stars: true,
        cover_link: true,
    };
    assign_data_to_work_with_client(db, rjcode.clone(), data_selection, Some(http_client)).await?;

    if let Ok(Some(cover_url)) = queries::get_cover_link(db, rjcode) {
        if let Err(e) = cover_art::download_cover_to_cache(&cover_url, &rjcode.to_string(), Some((500, 500))).await {
            warn!("Failed to cache fresh cover for {}: {}", rjcode, e);
        }
    }
    Ok(())
}

/// Phase 2 of a refresh (no network needed): applies the cached cover (forcing it to replace any
/// existing one) and re-tags the actual audio files (auto-converting FLAC/WAV/OGG to MP3 first).
/// Must only run after the VPN has been disconnected — this is what touches the real files, which
/// may live on a network share that's only reachable once the VPN tunnel is torn back down.
async fn apply_cover_and_tag(
    db: &rusqlite::Connection,
    rjcode: &RJCode,
    folder_path: String,
    app_config: &Config,
    write_tagged_marker: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let folder_path_obj = Path::new(&folder_path);
    let cover_path = folder_path_obj.join("folder.jpeg");
    if cover_path.exists() {
        std::fs::remove_file(&cover_path)?;
    }
    if let Err(e) = cover_art::copy_cover_from_cache(&rjcode.to_string(), folder_path_obj) {
        debug!("No fresh cached cover applied for {}: {}", rjcode, e);
    }

    let folder = ManagedFolder::new(folder_path);
    let tagger_config = TaggerConfig {
        tag_separator: app_config.tagger.get_separator(),
        convert_to_mp3: true,
        target_bitrate: 320,
        download_cover: true,
        force_retag: true,
        write_tagged_marker,
    };
    process_work_folder(db, &folder, &tagger_config).await?;
    Ok(())
}

/// `--retag <rjcode>`: refresh a single work already registered in the library.
async fn run_retag_workflow(
    db: &rusqlite::Connection,
    rjcode: &str,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let rjcode = RJCode::new(rjcode.to_string())?;
    let folder_path = queries::get_work_path(db, &rjcode)?
        .ok_or_else(|| format!(
            "{} not found in the database. Use --tag on its folder in the import directory instead.",
            rjcode
        ))?;

    if !converter::is_ffmpeg_available() {
        return Err("ffmpeg not found in PATH (required for automatic FLAC/WAV/OGG conversion).".into());
    }

    info!("=== RETAG {} ===", rjcode);

    let vpn_manager = connect_vpn_if_enabled(app_config)?;
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let metadata_result = refresh_metadata_and_cache_cover(db, &rjcode, &http_client).await;

    disconnect_vpn(vpn_manager)?;
    metadata_result?;

    apply_cover_and_tag(db, &rjcode, folder_path, app_config, true).await?;

    info!("=== RETAG COMPLETE: {} ===", rjcode);
    Ok(())
}

/// `--full-retag`: refresh EVERY work already registered in the library — same per-work refresh
/// as `--retag`, looped over the whole database. Connects the VPN once for the entire batch
/// rather than once per work (reconnecting per work would be needlessly slow for hundreds of
/// works). Continues past individual failures (e.g. a work whose folder no longer exists on
/// disk) so one bad work doesn't abort the whole batch; failures are reported in the summary.
async fn run_full_retag_workflow(
    db: &rusqlite::Connection,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    if !converter::is_ffmpeg_available() {
        return Err("ffmpeg not found in PATH (required for automatic FLAC/WAV/OGG conversion).".into());
    }

    let works = queries::get_all_works_with_paths(db)?;
    if works.is_empty() {
        info!("No works in database");
        return Ok(());
    }

    info!("=== FULL RETAG: {} work(s) ===", works.len());

    // ===== VPN PHASE: refresh DB metadata + cache fresh covers for every work =====
    // Only the database and the cover cache are touched here, exactly like `--full`'s collect
    // phase — the VPN is torn down before any of the actual work folders are touched below.
    let vpn_manager = connect_vpn_if_enabled(app_config)?;
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    info!("\n--- Fetching metadata ({} work(s)) ---", works.len());
    let pb = create_progress_bar(works.len() as u64);
    let mut metadata_ok: Vec<bool> = Vec::with_capacity(works.len());

    for (rjcode, _) in &works {
        pb.set_message(format!("Fetching {}", rjcode));
        match refresh_metadata_and_cache_cover(db, rjcode, &http_client).await {
            Ok(_) => {
                pb.println(format!("{} ✓", rjcode));
                metadata_ok.push(true);
            }
            Err(e) => {
                warn!("Failed to refresh metadata for {}: {}", rjcode, e);
                pb.println(format!("{} ✗", rjcode));
                metadata_ok.push(false);
            }
        }
        pb.inc(1);
    }
    pb.finish_and_clear();

    disconnect_vpn(vpn_manager)?;

    // ===== POST-VPN PHASE: apply cached covers + re-tag files, VPN is down =====
    info!("\n--- Tagging files ({} work(s)) ---", works.len());
    let pb = create_progress_bar(works.len() as u64);
    let mut success = 0usize;
    let mut failed = 0usize;

    for ((rjcode, folder_path), was_ok) in works.into_iter().zip(metadata_ok.into_iter()) {
        pb.set_message(format!("Tagging {}", rjcode));

        if !was_ok {
            // Metadata refresh already failed for this work; skip tagging and count it once.
            pb.println(format!("{} ✗ (metadata fetch failed)", rjcode));
            failed += 1;
            pb.inc(1);
            continue;
        }

        match apply_cover_and_tag(db, &rjcode, folder_path, app_config, true).await {
            Ok(_) => {
                pb.println(format!("{} ✓", rjcode));
                success += 1;
            }
            Err(e) => {
                warn!("Failed to tag {}: {}", rjcode, e);
                pb.println(format!("{} ✗", rjcode));
                failed += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();

    info!("=== FULL RETAG COMPLETE: {} succeeded, {} failed ===", success, failed);
    Ok(())
}

/// `--tag <folder_name>`: one-shot test run of the full process against a folder sitting in the
/// import directory — collects DLSite metadata, downloads a cover, tags the files (converting
/// FLAC/WAV/OGG first) — but does NOT move the folder and does NOT leave anything in the
/// database. The folder is registered temporarily so the existing DLSite-fetch and
/// custom-mapping-merge machinery (all keyed on fld_id) works unmodified, then fully removed
/// again at the end regardless of success or failure.
async fn run_tag_test_workflow(
    db: &rusqlite::Connection,
    folder_name: &str,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_path = app_config.import.source_path.as_ref()
        .ok_or("import.source_path is not configured in config.toml")?;
    let folder_path = Path::new(source_path).join(folder_name);
    if !folder_path.is_dir() {
        return Err(format!("Folder not found in import directory: {}", folder_path.display()).into());
    }

    let folder = ManagedFolder::new(folder_path.to_string_lossy().to_string());
    if !folder.is_valid {
        return Err(format!(
            "'{}' is not a valid work folder (needs an RJ/VJ-prefixed name and audio files)",
            folder_name
        ).into());
    }

    if queries::rjcode_exists(db, &folder.rjcode)? {
        return Err(format!(
            "{} is already registered in the database — use --retag {} instead.",
            folder.rjcode, folder.rjcode
        ).into());
    }

    if !converter::is_ffmpeg_available() {
        return Err("ffmpeg not found in PATH (required for automatic FLAC/WAV/OGG conversion).".into());
    }

    info!("=== TAG TEST (one-shot, no DB/move): {} ===", folder.rjcode);

    register_folders(db, vec![folder.clone()])?;

    let result = run_tag_test_inner(db, &folder, app_config).await;

    // Cleanup regardless of success/failure. Shared reference rows (dlsite_tag/circles/cvs
    // themselves) are correctly left untouched — only this fld_id's lkp_* rows disappear.
    queries::delete_work_permanently(db, &folder.rjcode)?;

    result?;
    info!(
        "=== TAG TEST COMPLETE: {}. Files updated in place; not moved, database not modified. ===",
        folder.rjcode
    );
    Ok(())
}

async fn run_tag_test_inner(
    db: &rusqlite::Connection,
    folder: &ManagedFolder,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let vpn_manager = connect_vpn_if_enabled(app_config)?;
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let metadata_result = refresh_metadata_and_cache_cover(db, &folder.rjcode, &http_client).await;

    disconnect_vpn(vpn_manager)?;
    metadata_result?;

    apply_cover_and_tag(db, &folder.rjcode, folder.path.clone(), app_config, false).await?;
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

    // ========== VPN PHASE ==========
    // --full always collects metadata and downloads covers, so VPN is always needed.
    let needs_vpn = true;
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

    // Collect metadata (--full always does this)
    {
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

    // Download covers (--full always does this)
    {
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
    {
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

    // Tag files (--full always does this)
    {
        info!("\n--- Tagging files ---");
        let tagger_config = TaggerConfig {
            tag_separator: app_config.tagger.get_separator(),
            convert_to_mp3: false,
            target_bitrate: 320,
            download_cover: true,
            force_retag: false,
            write_tagged_marker: true,
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
