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
    let needs_retag = needs_retag_tags || needs_retag_circle || config.force_retag;

    // Skip if already tagged and no re-tagging needed
    if folder.is_tagged && !needs_retag {
        debug!("Folder already tagged, skipping (use --force to re-tag)");
        return Ok(());
    }

    if config.force_retag {
        info!("Force re-tagging: {}", folder.rjcode.as_str());
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
    // Query database for work metadata (with fallback to RJCode if not collected yet)
    let work_name: String = conn.query_row(
        "SELECT name FROM works WHERE fld_id = (SELECT fld_id FROM folders WHERE rjcode = ?1)",
        rusqlite::params![rjcode],
        |row| row.get(0),
    ).unwrap_or_else(|_| {
        // Fallback: use RJCode as title if metadata not collected yet
        debug!("No metadata found for {}, using RJCode as title", rjcode);
        rjcode.to_string()
    });

    // Get circle name (with custom preference support)
    let circle_name = crate::database::custom_circles::get_merged_circle_name_for_work(conn, rjcode)
        .unwrap_or_else(|_| String::from("Unknown"));

    // Get tags (merged: DLSite + custom replacements) - returns empty vec if none
    let tags = crate::database::custom_tags::get_merged_tags_for_work(conn, rjcode)
        .unwrap_or_default();

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

    // STEP 0: Convert non-MP3 files if --convert is enabled
    if config.convert_to_mp3 {
        let entries = std::fs::read_dir(folder_path)?;
        for entry in entries {
            let entry = entry?;
            let file_path = entry.path();

            if !file_path.is_file() {
                continue;
            }

            let extension = file_path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let format = AudioFormat::from_extension(extension);

            // Convert FLAC, WAV, OGG to MP3
            if format == AudioFormat::Flac || format == AudioFormat::Wav || format == AudioFormat::Ogg {
                let filename = file_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                info!("Converting to MP3: {}", filename);

                match converter::convert_to_mp3_in_place(&file_path, config.target_bitrate).await {
                    Ok(_) => info!("Converted: {} -> .mp3", filename),
                    Err(e) => warn!("Failed to convert {}: {}", filename, e),
                }
            }
        }
    }

    // STEP 1: Collect all MP3 files
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

    // STEP 2: Check if files already have track numbers in their ID3 tags
    let mut existing_track_count = 0;
    for (file_path, _) in &audio_files {
        if let Ok(Some(existing_metadata)) = id3_handler::read_id3_tags(file_path) {
            if existing_metadata.track_number.is_some() {
                existing_track_count += 1;
            }
        }
    }

    // If most files already have track numbers, skip interactive parsing
    let existing_track_rate = existing_track_count as f32 / audio_files.len() as f32;
    let files_already_numbered = existing_track_rate > 0.7; // 70% threshold

    if files_already_numbered {
        debug!("Files already have track numbers ({}/{}), skipping track parsing prompt",
               existing_track_count, audio_files.len());
    }

    // STEP 3: Try to get saved parsing preference
    let parsing_pref = crate::database::queries::get_track_parsing_preference(conn, &folder.rjcode)?;

    // STEP 4: Test if we can parse track numbers from filenames
    let filenames: Vec<String> = audio_files.iter()
        .map(|(_, name)| name.clone())
        .collect();

    let mut current_pref = parsing_pref;
    // Per-file track numbers from manual input (Session-only, not saved to DB).
    let mut manual_numbers: Option<Vec<Option<u32>>> = None;

    // Only trigger interactive session when files don't already have numbers and no
    // saved preference exists, and automatic parsing fails on more than 30% of files.
    if !files_already_numbered && current_pref.is_none() {
        let parsed: Vec<Option<u32>> = filenames.iter()
            .map(|f| track_parser::parse_track_number(f))
            .collect();

        let failure_count = parsed.iter().filter(|p| p.is_none()).count();
        let failure_rate = failure_count as f32 / parsed.len() as f32;

        if failure_rate > 0.3 {
            info!("Automatic track parsing low confidence ({}/{} failed), requesting user input...",
                  failure_count, filenames.len());

            match interactive_parser::run_interactive_parsing(&filenames, folder.rjcode.as_str()) {
                Ok(interactive_parser::ParsingResult::Strategy(pref)) => {
                    crate::database::queries::save_track_parsing_preference(conn, &folder.rjcode, &pref)?;
                    info!("Track parsing preference saved for future use");
                    current_pref = Some(pref);
                }
                Ok(interactive_parser::ParsingResult::Manual(numbers)) => {
                    info!("Using manual track numbers for {}", folder.rjcode);
                    manual_numbers = Some(numbers);
                }
                Ok(interactive_parser::ParsingResult::Skip) => {
                    info!("Track numbering skipped for {}", folder.rjcode);
                }
                Err(e) => {
                    warn!("Interactive parsing failed: {}", e);
                }
            }
        }
    }

    // STEP 5: Tag each file
    for (file_index, (file_path, filename)) in audio_files.iter().enumerate() {
        let existing_track = if let Ok(Some(existing_metadata)) = id3_handler::read_id3_tags(file_path) {
            existing_metadata.track_number
        } else {
            None
        };

        let track_number = if let Some(ref nums) = manual_numbers {
            // Manual numbers override everything — the user chose each one explicitly
            nums.get(file_index).copied().flatten()
        } else if let Some(existing) = existing_track {
            debug!("File {} already has track number: {}, keeping it", filename, existing);
            Some(existing)
        } else {
            track_parser::parse_track_number_with_preference(filename, current_pref.as_ref())
        };

        let mut file_metadata = base_metadata.clone();
        file_metadata.track_number = track_number;
        file_metadata.title = track_parser::extract_track_title(filename);

        debug!("Tagging: {} (track: {:?}, title: {})", filename, track_number, file_metadata.title);

        let format = AudioFormat::Mp3;
        tag_audio_file(file_path, &file_metadata, &format, &config.tag_separator).await?;
        record_file_processing(conn, fld_id, file_path)?;
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

