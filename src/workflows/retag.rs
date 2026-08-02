//! `--retag`: refreshes a single work already registered in the library — re-collects DLSite
//! metadata (tags/circle/CVs/rating/stars/release_date), re-downloads its cover, and re-tags its
//! audio files. Frontend-agnostic — reports through `ProgressSink` and asks through
//! `InteractionProvider`, mirroring `workflows::import`, so the same pipeline drives both the CLI
//! (`main.rs`) and a web-triggered rescan job (`web::jobs`).

use std::path::Path;

use crate::config::{Config, VpnProvider};
use crate::database::queries;
use crate::dlsite::{assign_data_to_work_with_client, DataSelection};
use crate::errors::HvtError;
use crate::folders::types::{ManagedFolder, RJCode};
use crate::interaction::progress::ProgressSink;
use crate::interaction::InteractionProvider;
use crate::tagger::{cover_art, process_work_folder, types::TaggerConfig};
use crate::vpn::WireGuardManager;

/// Refreshes a single work: connect VPN (if needed) -> re-fetch metadata + cache a fresh cover
/// -> disconnect VPN -> apply the cached cover and re-tag the files on disk.
pub async fn run_retag_workflow(
    db: &rusqlite::Connection,
    rjcode: &RJCode,
    app_config: &Config,
    progress: &dyn ProgressSink,
    interaction: &dyn InteractionProvider,
) -> Result<(), HvtError> {
    let folder_path = queries::get_work_path(db, app_config, rjcode)?.ok_or_else(|| {
        HvtError::Generic(format!(
            "{} not found in the database. Use --tag on its folder in the import directory instead.",
            rjcode
        ))
    })?;

    progress.phase(&format!("Refreshing {}", rjcode));

    // ========== VPN PHASE ==========
    let mut vpn_manager: Option<WireGuardManager> = None;
    if app_config.vpn.enabled {
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
            VpnProvider::Proxy => {
                progress.log("Using configured proxy for DLSite access (no local VPN tunnel to manage)")
            }
            _ => progress.log(&format!(
                "VPN provider {:?} not implemented",
                app_config.vpn.provider
            )),
        }
    }

    let http_client = crate::vpn::build_dlsite_client(app_config)?;

    // Fetch metadata (--retag always does this)
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
    let metadata_result =
        assign_data_to_work_with_client(db, rjcode.clone(), data_selection, Some(&http_client)).await;

    // Cache a fresh cover — best-effort, doesn't fail the whole refresh on its own.
    if metadata_result.is_ok() {
        progress.phase("Downloading cover");
        match queries::get_cover_link(db, rjcode) {
            Ok(Some(cover_url)) => {
                if let Err(e) = cover_art::download_cover_to_cache(
                    &cover_url,
                    &rjcode.to_string(),
                    Some((500, 500)),
                    Some(&http_client),
                )
                .await
                {
                    progress.log(&format!("Failed to cache fresh cover for {}: {}", rjcode, e));
                }
            }
            _ => progress.log(&format!("No cover link available for {}", rjcode)),
        }
    }

    // Disconnect VPN before filesystem operations
    drop(vpn_manager);

    metadata_result?;

    // ========== POST-VPN PHASE ==========
    progress.phase("Tagging files");

    let folder_path_obj = Path::new(&folder_path);
    let cover_path = folder_path_obj.join("folder.jpeg");
    if cover_path.exists() {
        std::fs::remove_file(&cover_path)?;
    }
    if let Err(e) = cover_art::copy_cover_from_cache(&rjcode.to_string(), folder_path_obj) {
        progress.log(&format!("No fresh cached cover applied for {}: {}", rjcode, e));
    }

    let folder = ManagedFolder::new(folder_path);
    let tagger_config = TaggerConfig {
        tag_separator: app_config.tagger.get_separator(),
        convert_to_mp3: true,
        target_bitrate: 320,
        download_cover: true,
        force_retag: true,
        write_tagged_marker: true,
    };
    process_work_folder(db, &folder, &tagger_config, interaction).await?;

    progress.log(&format!("{} refreshed", rjcode));
    Ok(())
}
