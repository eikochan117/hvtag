//! `--full` import workflow: scan source -> collect metadata/covers (VPN) -> tag -> move to
//! library. Frontend-agnostic — reports through `ProgressSink` and asks through
//! `InteractionProvider`, so the same pipeline drives both the CLI (`main.rs`) and a future
//! web-triggered job (`web::jobs`).

use std::path::Path;

use crate::config::{Config, VpnProvider};
use crate::database::queries;
use crate::dlsite::{assign_data_to_work_with_client, DataSelection};
use crate::errors::HvtError;
use crate::folders::{get_list_of_folders, register_folders, types::ManagedFolder};
use crate::interaction::progress::ProgressSink;
use crate::interaction::InteractionProvider;
use crate::tagger::{cover_art, folder_normalizer, process_work_folder, types::TaggerConfig};
use crate::vpn::WireGuardManager;

/// Import workflow: scan source -> process -> move to library.
pub async fn run_import_workflow(
    db: &rusqlite::Connection,
    app_config: &Config,
    progress: &dyn ProgressSink,
    interaction: &dyn InteractionProvider,
) -> Result<(), HvtError> {
    // Validate config
    let source_path = app_config.import.source_path.as_ref().ok_or_else(|| {
        HvtError::Generic("Please configure import.source_path in config.toml".to_string())
    })?;
    let library_path = app_config.import.library_path.as_ref().ok_or_else(|| {
        HvtError::Generic("Please configure import.library_path in config.toml".to_string())
    })?;

    progress.phase("Import workflow");
    progress.log(&format!("Source: {}", source_path));
    progress.log(&format!("Library: {}", library_path));

    // ========== REMOTE SOURCES ==========
    // Pull any configured remote drop folders into source_path before scanning it. No-op (no
    // phase reported) when import.remote_sources is empty.
    crate::workflows::remote_sync::collect_remote_sources(app_config, progress).await?;

    // ========== PRE-VPN PHASE ==========
    // 1. Prepare source folders: rename non-RJ roots and flatten audio files
    progress.phase("Preparing source folders");
    match folder_normalizer::prepare_source_directory(source_path) {
        Ok(0) => {}
        Ok(n) => progress.log(&format!("Prepared {} folder(s)", n)),
        Err(e) => progress.log(&format!("Folder preparation encountered an error: {}", e)),
    }

    // 2. Scan source directory
    progress.phase("Scanning source directory");
    let source_folders = get_list_of_folders(source_path)?;

    if source_folders.is_empty() {
        progress.log("No valid RJ folders found in source directory");
        return Ok(());
    }

    progress.log(&format!("Found {} folder(s) to import", source_folders.len()));

    // 2. Filter out folders that already exist in library
    let library_path_obj = Path::new(library_path);
    if !library_path_obj.exists() {
        std::fs::create_dir_all(library_path_obj)?;
        progress.log(&format!("Created library directory: {}", library_path));
    }

    let mut folders_to_process: Vec<ManagedFolder> = Vec::new();
    for folder in source_folders {
        let folder_name = Path::new(&folder.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let target_path = library_path_obj.join(folder_name);

        if target_path.exists() {
            progress.log(&format!("{} already exists in library, skipping", folder.rjcode));
        } else {
            folders_to_process.push(folder);
        }
    }

    if folders_to_process.is_empty() {
        progress.log("All folders already exist in library, nothing to import");
        return Ok(());
    }

    progress.log(&format!("{} folder(s) to process", folders_to_process.len()));

    // Register folders in DB now (with source path) so that --collect and --tag can resolve
    // fld_id during this same run. The path will be updated to the library path after the move.
    progress.phase("Registering folders in database");
    for folder in &folders_to_process {
        if let Err(e) = register_folders(db, vec![folder.clone()]) {
            progress.log(&format!("Failed to register {} in DB: {}", folder.rjcode, e));
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
                        progress.log("VPN already connected, reusing");
                    } else {
                        progress.log("Connecting VPN...");
                        manager.connect()?;
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }

                    vpn_manager = Some(manager);
                }
            }
            VpnProvider::Proxy => progress.log(
                "Using configured proxy for DLSite access (no local VPN tunnel to manage)",
            ),
            _ => progress.log(&format!(
                "VPN provider {:?} not implemented",
                app_config.vpn.provider
            )),
        }
    }

    // Create HTTP client (routed through vpn.proxy for provider = "proxy")
    let http_client = crate::vpn::build_dlsite_client(app_config)?;

    // Collect metadata (--full always does this)
    {
        progress.phase("Fetching metadata");
        let data_selection = DataSelection {
            tags: true,
            release_date: true,
            circle: true,
            rating: true,
            cvs: true,
            stars: true,
            cover_link: true,
        };

        progress.start_step(folders_to_process.len() as u64);

        for folder in &folders_to_process {
            let result_msg = match assign_data_to_work_with_client(
                db,
                folder.rjcode.clone(),
                data_selection.clone(),
                Some(&http_client),
            )
            .await
            {
                Ok(_) => format!("{} ✓", folder.rjcode),
                Err(HvtError::RemovedWork(rjcode)) => {
                    queries::insert_error(db, &rjcode, "removed work", Some("dlsite_removed"))?;
                    format!("{} (removed)", folder.rjcode)
                }
                Err(e) => {
                    progress.log(&format!("Error fetching {}: {}", folder.rjcode, e));
                    format!("{} ✗", folder.rjcode)
                }
            };

            progress.item(&result_msg);
        }

        progress.finish_step();
    }

    // Download covers (--full always does this)
    {
        progress.phase("Downloading covers");

        // Filter folders that need covers (don't have folder.jpeg yet)
        let folders_needing_covers: Vec<_> = folders_to_process
            .iter()
            .filter(|f| !cover_art::has_cover_art(Path::new(&f.path)))
            .collect();

        if folders_needing_covers.is_empty() {
            progress.log("All folders already have covers, skipping download");
        } else {
            progress.log(&format!("{} folder(s) need covers", folders_needing_covers.len()));
            progress.start_step(folders_needing_covers.len() as u64);

            for folder in &folders_needing_covers {
                let result_msg = match queries::get_cover_link(db, &folder.rjcode) {
                    Ok(Some(cover_url)) => {
                        match cover_art::download_cover_to_cache(
                            &cover_url,
                            &folder.rjcode.to_string(),
                            Some((500, 500)),
                        )
                        .await
                        {
                            Ok(_) => format!("{} cover ✓", folder.rjcode),
                            Err(e) => {
                                progress.log(&format!(
                                    "Failed to download cover for {}: {}",
                                    folder.rjcode, e
                                ));
                                format!("{} cover ✗", folder.rjcode)
                            }
                        }
                    }
                    _ => format!("{} cover ✗ (no link)", folder.rjcode),
                };

                progress.item(&result_msg);
            }

            progress.finish_step();
        }
    }

    // Disconnect VPN before filesystem operations
    drop(vpn_manager);

    // ========== POST-VPN PHASE ==========

    // Copy covers from cache to source folders (only for folders that don't have covers)
    {
        progress.phase("Copying covers to folders");
        for folder in &folders_to_process {
            let folder_path = Path::new(&folder.path);

            // Skip if folder already has a cover
            if cover_art::has_cover_art(folder_path) {
                continue;
            }

            let _ = cover_art::copy_cover_from_cache(&folder.rjcode.to_string(), folder_path);
        }
    }

    // Tag files (--full always does this)
    {
        progress.phase("Tagging files");
        let tagger_config = TaggerConfig {
            tag_separator: app_config.tagger.get_separator(),
            convert_to_mp3: false,
            target_bitrate: 320,
            download_cover: true,
            force_retag: false,
            write_tagged_marker: true,
        };

        progress.start_step(folders_to_process.len() as u64);

        for folder in &folders_to_process {
            let result_msg = match process_work_folder(db, folder, &tagger_config, interaction).await {
                Ok(_) => format!("{} tagged ✓", folder.rjcode),
                Err(e) => {
                    progress.log(&format!("Failed to tag {}: {}", folder.rjcode, e));
                    format!("{} tag ✗", folder.rjcode)
                }
            };

            progress.item(&result_msg);
        }

        progress.finish_step();
    }

    // Move folders to library and register in database
    progress.phase("Moving to library");
    progress.start_step(folders_to_process.len() as u64);
    let mut success_count = 0;
    let mut fail_count = 0;

    for folder in &folders_to_process {
        let source = Path::new(&folder.path);
        let folder_name = source
            .file_name()
            .ok_or_else(|| HvtError::Generic(format!("Invalid path: {}", folder.path)))?;
        let target = library_path_obj.join(folder_name);

        let result_msg = match move_folder_cross_drive(source, &target) {
            Ok(_) => {
                // Update path to final library location (folder was already registered earlier)
                let target_path_str = target.to_string_lossy().to_string();
                if let Err(e) = queries::update_folder_path(db, &folder.rjcode, &target_path_str) {
                    progress.log(&format!(
                        "Moved {} but failed to update path in DB: {}",
                        folder.rjcode, e
                    ));
                    fail_count += 1;
                    format!("{} ⚠ (DB path error)", folder.rjcode)
                } else {
                    success_count += 1;
                    format!("{} ✓", folder.rjcode)
                }
            }
            Err(e) => {
                progress.log(&format!("Failed to move {}: {}", folder.rjcode, e));
                fail_count += 1;
                format!("{} ✗", folder.rjcode)
            }
        };

        progress.item(&result_msg);
    }

    progress.finish_step();

    progress.phase("Import complete");
    progress.log(&format!("Imported: {} | Failed: {}", success_count, fail_count));

    Ok(())
}

/// Move folder with cross-drive support (copy + delete fallback)
pub fn move_folder_cross_drive(source: &Path, target: &Path) -> Result<(), HvtError> {
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
                copy_dir_recursive(source, target)?;
                std::fs::remove_dir_all(source).map_err(|e| {
                    HvtError::Generic(format!("Failed to remove source after copy: {}", e))
                })?;
                Ok(())
            } else {
                Err(HvtError::Generic(format!("Failed to move folder: {}", e)))
            }
        }
    }
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), HvtError> {
    std::fs::create_dir_all(dst)
        .map_err(|e| HvtError::Generic(format!("Failed to create directory {}: {}", dst.display(), e)))?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| HvtError::Generic(format!("Failed to read directory {}: {}", src.display(), e)))?
    {
        let entry = entry.map_err(|e| HvtError::Generic(format!("Failed to read entry: {}", e)))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                HvtError::Generic(format!(
                    "Failed to copy {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                ))
            })?;
        }
    }

    Ok(())
}
