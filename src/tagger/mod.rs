pub mod types;
pub mod track_parser;
pub mod cover_art;
pub mod id3_handler;
//pub mod flac_handler;
pub mod converter;
pub mod folder_normalizer;

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

    // Check if re-tagging needed (custom tags modified)
    let needs_retag = crate::database::custom_tags::should_retag_work(conn, &folder.rjcode).unwrap_or(false);

    // Skip if already tagged and no re-tagging needed
    if folder.is_tagged && !needs_retag {
        info!("Folder already tagged, skipping");
        return Ok(());
    }

    if needs_retag {
        info!("Custom tags modified, re-tagging work: {}", folder.rjcode.as_str());
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
                Some(config.cover_size),
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
) -> Result<(), HvtError> {
    match format {
        AudioFormat::Mp3 => {
            id3_handler::write_id3_tags(file_path, metadata)?;
        }
        AudioFormat::Flac => {
            todo!()
            //flac_handler::write_flac_tags(file_path, metadata)?;
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

    // Get circle name (prioritize JP over EN)
    let circle_name: String = conn.query_row(
        "SELECT COALESCE(NULLIF(name_jp, ''), name_en, 'Unknown Circle') FROM circles WHERE cir_id IN (
            SELECT cir_id FROM lkp_work_circle WHERE fld_id = (
                SELECT fld_id FROM folders WHERE rjcode = ?1
            )
        )",
        rusqlite::params![rjcode],
        |row| row.get(0),
    ).unwrap_or_else(|_| String::from("Unknown Circle"));

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
    let folder_path = Path::new(&folder.path);

    // After normalization, all audio files are at the root level
    // So we only need to scan the immediate directory
    let entries = std::fs::read_dir(folder_path)?;

    for entry in entries {
        let entry = entry?;
        let file_path = entry.path();

        // Only process files (not directories)
        if !file_path.is_file() {
            continue;
        }

        let extension = file_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let format = AudioFormat::from_extension(extension);

        // Only process MP3 and FLAC files
        if format != AudioFormat::Mp3 && format != AudioFormat::Flac {
            continue;
        }

        // Parse track number from filename
        let filename = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let track_number = track_parser::parse_track_number(filename);

        // Create metadata for this specific file
        let mut file_metadata = base_metadata.clone();
        file_metadata.track_number = track_number;

        debug!("Tagging: {} (track: {:?})", filename, track_number);

        // Tag the file
        tag_audio_file(&file_path, &file_metadata, &format).await?;

        // Record file processing in database
        record_file_processing(conn, fld_id, &file_path)?;

        // Convert if needed
        if config.convert_to_mp3 && format == AudioFormat::Flac {
            info!("Converting to MP3: {}", filename);
            converter::convert_to_mp3_in_place(&file_path, config.target_bitrate).await?;
        }
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

