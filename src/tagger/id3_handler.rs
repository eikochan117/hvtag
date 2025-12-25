use std::path::Path;
use id3::TagLike;
use crate::errors::HvtError;
use crate::tagger::types::AudioMetadata;

/// Writes ID3v2 tags to an MP3 file
/// Note: Cover art is NOT embedded - it's saved separately as folder.jpeg
pub fn write_id3_tags(file_path: &Path, metadata: &AudioMetadata) -> Result<(), HvtError> {
    let mut tag = match id3::Tag::read_from_path(file_path) {
        Ok(t) => t,
        Err(_) => id3::Tag::new(),
    };

    // Set basic metadata
    tag.set_title(&metadata.title);
    tag.set_album(&metadata.album);
    tag.set_album_artist(&metadata.album_artist);

    // Set artists (voice actors) - multiple artists separated by semicolon
    if !metadata.artists.is_empty() {
        let artists_string = metadata.artists.join(";");
        tag.set_artist(&artists_string);
    }

    // Set track number if available
    if let Some(track) = metadata.track_number {
        tag.set_track(track);
    }

    // Set date if available
    // Note: id3 crate's set_date_released expects specific format
    // For now, we skip this if the date string doesn't match expected format
    if let Some(_date) = &metadata.date {
        // TODO: Parse date string into id3::Timestamp format
        // Skipping for now as it requires specific date format parsing
    }

    // Set genre (concatenate all genres with semicolon)
    if !metadata.genre.is_empty() {
        let genre_string = metadata.genre.join(";");
        tag.set_genre(&genre_string);
    }

    // Write tags to file
    tag.write_to_path(file_path, id3::Version::Id3v24)
        .map_err(|e| HvtError::AudioTag(format!("Failed to write ID3 tags: {}", e)))?;

    Ok(())
}

/// Reads ID3v2 tags from an MP3 file
pub fn read_id3_tags(file_path: &Path) -> Result<Option<AudioMetadata>, HvtError> {
    let tag = match id3::Tag::read_from_path(file_path) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    // Get genre - id3 crate's genres() returns Option<Vec<&str>>
    let genres: Vec<String> = tag.genres()
        .unwrap_or_default()
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Parse artists (separated by semicolon)
    let artists_str = tag.artist().unwrap_or("");
    let artists: Vec<String> = if !artists_str.is_empty() {
        artists_str.split(';').map(|s| s.trim().to_string()).collect()
    } else {
        Vec::new()
    };

    let metadata = AudioMetadata {
        title: tag.title().unwrap_or("").to_string(),
        artists,
        album: tag.album().unwrap_or("").to_string(),
        album_artist: tag.album_artist().unwrap_or("").to_string(),
        track_number: tag.track(),
        genre: genres,
        date: tag.date_released().map(|d| d.to_string()),
    };

    Ok(Some(metadata))
}
