use std::path::Path;
use id3::TagLike;
use crate::errors::HvtError;
use crate::tagger::types::AudioMetadata;

/// Writes ID3v2 tags to an MP3 file
/// Note: Cover art is NOT embedded - it's saved separately as folder.jpeg
pub fn write_id3_tags(file_path: &Path, metadata: &AudioMetadata, separator: &str) -> Result<(), HvtError> {
    let mut tag = match id3::Tag::read_from_path(file_path) {
        Ok(t) => t,
        Err(_) => id3::Tag::new(),
    };

    // Strip any embedded cover art — hvtag keeps the cover as a separate folder.jpeg,
    // and embedded pictures from source files would otherwise survive re-tagging.
    tag.remove_all_pictures();

    // Set basic metadata
    tag.set_title(&metadata.title);
    tag.set_album(&metadata.album);
    tag.set_album_artist(&metadata.album_artist);

    // Set artists (voice actors) - multiple artists separated by configured separator
    if !metadata.artists.is_empty() {
        let artists_string = metadata.artists.join(separator);
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

    // Set genre (concatenate all genres with configured separator)
    if !metadata.genre.is_empty() {
        let genre_string = metadata.genre.join(separator);
        tag.set_genre(&genre_string);
    }

    // Write tags to file
    tag.write_to_path(file_path, id3::Version::Id3v24)
        .map_err(|e| HvtError::AudioTag(format!("Failed to write ID3 tags: {}", e)))?;

    Ok(())
}
