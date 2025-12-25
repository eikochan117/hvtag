use std::path::Path;
use crate::errors::HvtError;
use crate::tagger::types::AudioMetadata;

/// Writes Vorbis comments to a FLAC file
/// Note: Cover art is NOT embedded - it's saved separately as folder.jpeg
pub fn write_flac_tags(file_path: &Path, metadata: &AudioMetadata) -> Result<(), HvtError> {
    let mut tag = metaflac::Tag::read_from_path(file_path)
        .map_err(|e| HvtError::AudioTag(format!("Failed to read FLAC file: {}", e)))?;

    // Clear existing vorbis comments to avoid duplicates
    tag.remove_vorbis("TITLE");
    tag.remove_vorbis("ARTIST");
    tag.remove_vorbis("ALBUMARTIST");
    tag.remove_vorbis("ALBUM");
    tag.remove_vorbis("TRACKNUMBER");
    tag.remove_vorbis("DATE");
    tag.remove_vorbis("GENRE");

    // Set new tags
    tag.set_vorbis("TITLE", vec![&metadata.title]);
    tag.set_vorbis("ALBUM", vec![&metadata.album]);
    tag.set_vorbis("ALBUMARTIST", vec![&metadata.album_artist]);

    // Set artists (voice actors) - FLAC supports multiple artists natively
    if !metadata.artists.is_empty() {
        let artist_refs: Vec<&str> = metadata.artists.iter().map(|s| s.as_str()).collect();
        tag.set_vorbis("ARTIST", artist_refs);
    }

    if let Some(track) = metadata.track_number {
        tag.set_vorbis("TRACKNUMBER", vec![&track.to_string()]);
    }

    if let Some(date) = &metadata.date {
        tag.set_vorbis("DATE", vec![date]);
    }

    // Set genres (can set multiple)
    if !metadata.genre.is_empty() {
        let genre_refs: Vec<&str> = metadata.genre.iter().map(|s| s.as_str()).collect();
        tag.set_vorbis("GENRE", genre_refs);
    }

    // Save tags
    tag.save()
        .map_err(|e| HvtError::AudioTag(format!("Failed to save FLAC tags: {}", e)))?;

    Ok(())
}

/// Reads Vorbis comments from a FLAC file
pub fn read_flac_tags(file_path: &Path) -> Result<Option<AudioMetadata>, HvtError> {
    let tag = match metaflac::Tag::read_from_path(file_path) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    let get_vorbis = |key: &str| -> String {
        tag.get_vorbis(key)
            .and_then(|mut v| v.next())
            .unwrap_or("")
            .to_string()
    };

    let genres: Vec<String> = tag.get_vorbis("GENRE")
        .map(|iter| iter.map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let track_number = tag.get_vorbis("TRACKNUMBER")
        .and_then(|mut v| v.next())
        .and_then(|s| s.parse::<u32>().ok());

    let metadata = AudioMetadata {
        title: get_vorbis("TITLE"),
        artist: get_vorbis("ARTIST"),
        album: get_vorbis("ALBUM"),
        track_number,
        genre: genres,
        date: Some(get_vorbis("DATE")).filter(|s| !s.is_empty()),
        comment: get_vorbis("COMMENT"),
    };

    Ok(Some(metadata))
}
