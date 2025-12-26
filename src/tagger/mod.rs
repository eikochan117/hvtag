pub mod types;
pub mod track_parser;
pub mod cover_art;
pub mod id3_handler;
pub mod converter;
pub mod folder_normalizer;
pub mod interactive_parser;

use std::path::Path;
use rusqlite::Connection;
use tracing::{info, warn, debug};
use crate::errors::HvtError;
use crate::folders::types::{ManagedFolder, RJCode};
use crate::tagger::types::{AudioMetadata, TaggerConfig, AudioFormat};

/// Main function to process a work folder:
/// 1. Fetch metadata from database
/// 2. Download cover art (if enabled)
/// 3. Tag all audio files
/// 4. Convert to MP3 (if enabled)
/// 5. Mark folder as tagged
pub async fn process_work_folder(
    conn: &Connection,
    folder: &ManagedFolder,
    config: &TaggerConfig,
) -> Result<(), HvtError> {
    info!("Processing folder: {}", folder.path);

    // Check if re-tagging needed (custom tags OR circle preferences modified)
    let needs_retag_tags = crate::database::custom_tags::should_retag_work(conn, &folder.rjcode).unwrap_or(false);
    let needs_retag_circle = crate::database::custom_circles::should_retag_work_for_circle(conn, &folder.rjcode).unwrap_or(false);
    let needs_retag = needs_retag_tags || needs_retag_circle;

    // Skip if already tagged and no re-tagging needed
    if folder.is_tagged && !needs_retag {
        info!("Folder already tagged, skipping");
        return Ok(());
    }

    if needs_retag_tags {
        info!("Custom tags modified, re-tagging work: {}", folder.rjcode.as_str());
    }
    if needs_retag_circle {
        info!("Circle preference modified, re-tagging work: {}", folder.rjcode.as_str());
    }

    // Step 0: Normalize folder structure (move all audio files to root level)
    let folder_path = Path::new(&folder.path);
    match folder_normalizer::normalize_folder_structure(folder_path) {
        Ok(count) if count > 0 => info!("Normalized folder structure: {} files moved", count),
        Ok(_) => {}, // Already normalized
        Err(e) => warn!("Failed to normalize folder structure: {}", e),
    }

    // Get fld_id for this work
    let fld_id = get_fld_id(conn, &folder.rjcode)?;

    // Fetch metadata from database
    let metadata = fetch_metadata_from_db(conn, &folder.rjcode)?;

    // Download cover art if enabled and not already present
    if config.download_cover && !folder.has_cover {
        if let Some(cover_url) = get_cover_url(conn, &folder.rjcode)? {
            let folder_path = Path::new(&folder.path);
            match cover_art::download_and_save_cover(
                &cover_url,
                folder_path,
                None,  // Keep original dimensions from DLSite
            ).await {
                Ok(_) => info!("Cover art downloaded successfully"),
                Err(e) => warn!("Failed to download cover art: {}", e),
            }
        }
    }

    // Tag all audio files
    tag_all_files(conn, fld_id, folder, &metadata, config).await?;

    // Mark folder as tagged by creating .tagged file
    create_tagged_marker(&folder.path)?;

    info!("Successfully processed folder: {}", folder.path);
    Ok(())
}

/// Tags a single audio file based on its format
pub async fn tag_audio_file(
    file_path: &Path,
    metadata: &AudioMetadata,
    format: &AudioFormat,
    separator: &str,
) -> Result<(), HvtError> {
    match format {
        AudioFormat::Mp3 => {
            id3_handler::write_id3_tags(file_path, metadata, separator)?;
        }
        AudioFormat::Flac => {
            return Err(HvtError::AudioTag(
                format!("FLAC files are not supported for tagging. Please convert to MP3 first using --convert flag. File: {}",
                    file_path.display())
            ));
        }
        _ => {
            return Err(HvtError::AudioTag(
                format!("Unsupported audio format for file: {}", file_path.display())
            ));
        }
    }
    Ok(())
}

// Helper functions

fn fetch_metadata_from_db(conn: &Connection, rjcode: &RJCode) -> Result<AudioMetadata, HvtError> {
    // Query database for work metadata
    let work_name: String = conn.query_row(
        "SELECT name FROM works WHERE fld_id = (SELECT fld_id FROM folders WHERE rjcode = ?1)",
        rusqlite::params![rjcode],
        |row| row.get(0),
    ).map_err(|_| HvtError::Database(rusqlite::Error::QueryReturnedNoRows))?;

    // Get circle name (with custom preference support)
    let circle_name = crate::database::custom_circles::get_merged_circle_name_for_work(conn, rjcode)?;

    // Get tags (merged: DLSite + custom replacements)
    let tags = crate::database::custom_tags::get_merged_tags_for_work(conn, rjcode)?;

    // Get CVs (voice actors) - will be used as artists
    let mut cv_stmt = conn.prepare(
        "SELECT name_jp FROM cvs WHERE cv_id IN (
            SELECT cv_id FROM lkp_work_cvs WHERE fld_id = (
                SELECT fld_id FROM folders WHERE rjcode = ?1
            )
        )"
    )?;

    let cvs: Vec<String> = cv_stmt
        .query_map(rusqlite::params![rjcode], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Get release date
    let release_date: Option<String> = conn.query_row(
        "SELECT release_date FROM release_date WHERE fld_id = (
            SELECT fld_id FROM folders WHERE rjcode = ?1
        )",
        rusqlite::params![rjcode],
        |row| row.get(0),
    ).ok();

    Ok(AudioMetadata {
        title: work_name.clone(),
        artists: cvs,              // Voice actors as artists
        album: work_name,
        album_artist: circle_name, // Circle as album artist
        track_number: None,        // Will be set per-file
        genre: tags,
        date: release_date,
    })
}

fn get_cover_url(conn: &Connection, rjcode: &RJCode) -> Result<Option<String>, HvtError> {
    let url: Option<String> = conn.query_row(
        "SELECT link FROM dlsite_covers WHERE fld_id = (
            SELECT fld_id FROM folders WHERE rjcode = ?1
        )",
        rusqlite::params![rjcode],
        |row| row.get(0),
    ).ok();

    Ok(url)
}

async fn tag_all_files(
    conn: &Connection,
    fld_id: i64,
    folder: &ManagedFolder,
    base_metadata: &AudioMetadata,
    config: &TaggerConfig,
) -> Result<(), HvtError> {
    use std::path::PathBuf;

    let folder_path = Path::new(&folder.path);

    // STEP 1: Collect all MP3 files first
    let entries = std::fs::read_dir(folder_path)?;
    let mut audio_files: Vec<(PathBuf, String)> = Vec::new();

    for entry in entries {
        let entry = entry?;
        let file_path = entry.path();

        if !file_path.is_file() {
            continue;
        }

        let filename = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let extension = file_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let format = AudioFormat::from_extension(extension);

        // Only process MP3 files
        if format != AudioFormat::Mp3 {
            if format == AudioFormat::Flac || format == AudioFormat::Wav || format == AudioFormat::Ogg {
                warn!("Skipping non-MP3 file: {}. Use --convert to convert to MP3 first.", filename);
            }
            continue;
        }

        audio_files.push((file_path, filename));
    }

    if audio_files.is_empty() {
        warn!("No MP3 files found in folder");
        return Ok(());
    }

    // STEP 2: Try to get saved parsing preference
    let parsing_pref = crate::database::queries::get_track_parsing_preference(conn, &folder.rjcode)?;

    // STEP 3: Test if we can parse track numbers
    let filenames: Vec<String> = audio_files.iter()
        .map(|(_, name)| name.clone())
        .collect();

    let mut current_pref = parsing_pref;
    let mut need_user_input = false;

    // If no saved preference, try automatic parsing
    if current_pref.is_none() {
        let parsed: Vec<Option<u32>> = filenames.iter()
            .map(|f| track_parser::parse_track_number(f))
            .collect();

        let failure_count = parsed.iter().filter(|p| p.is_none()).count();
        let failure_rate = failure_count as f32 / parsed.len() as f32;

        // If more than 30% failed, ask user
        if failure_rate > 0.3 {
            need_user_input = true;
        }
    }

    // STEP 4: Interactive prompt if needed
    if need_user_input {
        info!("Automatic track parsing has low confidence, requesting user input...");

        match interactive_parser::prompt_for_parsing_strategy(&filenames, folder.rjcode.as_str()) {
            Ok(pref) => {
                // Test the strategy
                let test_results = interactive_parser::test_strategy(&filenames, &pref);

                // Show preview and get confirmation
                match interactive_parser::confirm_strategy(&filenames, &test_results) {
                    Ok(true) => {
                        // Save preference
                        crate::database::queries::save_track_parsing_preference(conn, &folder.rjcode, &pref)?;
                        current_pref = Some(pref);
                        info!("Track parsing preference saved for future use");
                    }
                    Ok(false) => {
                        warn!("Strategy rejected by user, continuing without track numbers");
                    }
                    Err(e) => {
                        warn!("Confirmation error: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("User skipped interactive parsing: {}", e);
            }
        }
    }

    // STEP 5: Process each file with the preference
    for (file_path, filename) in audio_files {
        let track_number = track_parser::parse_track_number_with_preference(
            &filename,
            current_pref.as_ref(),
        );

        let mut file_metadata = base_metadata.clone();
        file_metadata.track_number = track_number;

        debug!("Tagging: {} (track: {:?})", filename, track_number);

        let format = AudioFormat::Mp3;
        tag_audio_file(&file_path, &file_metadata, &format, &config.tag_separator).await?;
        record_file_processing(conn, fld_id, &file_path)?;

        // Note: Convert is only for FLAC, which we already filtered out
    }

    Ok(())
}

fn create_tagged_marker(folder_path: &str) -> Result<(), HvtError> {
    let marker_path = Path::new(folder_path).join(".tagged");
    std::fs::write(marker_path, "")?;
    Ok(())
}

/// Record file processing in database
fn record_file_processing(
    conn: &Connection,
    fld_id: i64,
    file_path: &Path,
) -> Result<(), HvtError> {
    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let file_size = std::fs::metadata(file_path).map(|m| m.len() as i64).unwrap_or(0);

    conn.execute(
        "INSERT OR REPLACE INTO file_processing
         (fld_id, file_path, file_name, file_extension, file_size_bytes,
          is_tagged, tag_date, last_processed, processing_status)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, datetime('now'), datetime('now'), 'completed')",
        rusqlite::params![fld_id, file_path.display().to_string(), file_name, extension, file_size],
    )?;

    Ok(())
}

/// Get fld_id for a work
fn get_fld_id(conn: &Connection, rjcode: &RJCode) -> Result<i64, HvtError> {
    let fld_id: i64 = conn.query_row(
        "SELECT fld_id FROM folders WHERE rjcode = ?1",
        rusqlite::params![rjcode.as_str()],
        |row| row.get(0),
    )?;
    Ok(fld_id)
}

