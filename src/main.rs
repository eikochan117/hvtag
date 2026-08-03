
use clap::Parser;
use tracing::{info, warn, debug};
use indicatif::{ProgressBar, ProgressStyle, ProgressDrawTarget};

use std::path::Path;
use crate::{
    database::{db_loader::open_db, init, queries},
    dlsite::{assign_data_to_work_with_client, DataSelection},
    folders::{register_folders, types::{ManagedFolder, RJCode}},
    tagger::{cover_art, converter, process_work_folder, types::TaggerConfig},
    vpn::WireGuardManager,
    config::Config,
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
mod interaction;
mod workflows;
mod paths;

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

    info!(
        "hvtag v{} (built {})",
        env!("CARGO_PKG_VERSION"),
        env!("HVTAG_BUILD_DATE")
    );

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
        let progress = interaction::cli::CliProgressSink::new();
        let interaction_provider = interaction::cli::CliInteractionProvider::new();
        workflows::import::run_import_workflow(&db, &app_config, &progress, &interaction_provider).await?;
        return Ok(());
    }

    info!("No action specified. Use --full to import new works, --retag <rjcode> to refresh an existing work, --tag <folder> to test-tag a folder without importing it, or --ui to browse the library.");
    Ok(())
}

/// Connects the configured VPN if enabled, reusing an already-active tunnel if present.
/// Used by `--retag`/`--tag`, which each need one DLSite fetch surrounded by connect/disconnect.
///
/// Only `provider = "wireguard"` has a tunnel for hvtag to manage here. `provider = "proxy"`
/// has nothing to connect — the HTTP client itself is routed through the configured proxy (see
/// `vpn::build_dlsite_client`), so this returns `None` and the caller's disconnect is a no-op.
fn connect_vpn_if_enabled(app_config: &Config) -> Result<Option<WireGuardManager>, Box<dyn std::error::Error>> {
    if !app_config.vpn.enabled || !matches!(app_config.vpn.provider, crate::config::VpnProvider::Wireguard) {
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
        if let Err(e) = cover_art::download_cover_to_cache(&cover_url, &rjcode.to_string(), Some((500, 500)), Some(http_client)).await {
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
    let interaction = crate::interaction::cli::CliInteractionProvider::new();
    process_work_folder(db, &folder, &tagger_config, &interaction).await?;
    Ok(())
}

/// `--retag <rjcode>`: refresh a single work already registered in the library. Thin CLI wrapper
/// around the shared `workflows::retag::run_retag_workflow` (also used by the web UI's
/// per-work "rescan" button).
async fn run_retag_workflow(
    db: &rusqlite::Connection,
    rjcode: &str,
    app_config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let rjcode = RJCode::new(rjcode.to_string())?;

    if !converter::is_ffmpeg_available() {
        return Err("ffmpeg not found in PATH (required for automatic FLAC/WAV/OGG conversion).".into());
    }

    info!("=== RETAG {} ===", rjcode);

    let progress = interaction::cli::CliProgressSink::new();
    let interaction_provider = interaction::cli::CliInteractionProvider::new();
    workflows::retag::run_retag_workflow(db, &rjcode, app_config, &progress, &interaction_provider).await?;

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

    let works = queries::get_all_works_with_paths(db, app_config)?;
    if works.is_empty() {
        info!("No works in database");
        return Ok(());
    }

    info!("=== FULL RETAG: {} work(s) ===", works.len());

    // ===== VPN PHASE: refresh DB metadata + cache fresh covers for every work =====
    // Only the database and the cover cache are touched here, exactly like `--full`'s collect
    // phase — the VPN is torn down before any of the actual work folders are touched below.
    let vpn_manager = connect_vpn_if_enabled(app_config)?;
    let http_client = vpn::build_dlsite_client(app_config)?;

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

    register_folders(db, app_config, vec![folder.clone()])?;

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
    let http_client = vpn::build_dlsite_client(app_config)?;

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

