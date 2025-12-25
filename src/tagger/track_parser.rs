use regex::Regex;

/// Parses track number from filename with support for multiple naming patterns
///
/// Supports:
/// - "01 - Track.mp3"
/// - "01.Track.mp3"
/// - "prefix_01_Track.mp3"
/// - "#3-A.titre.mp3"
/// - "Track 01.mp3"
/// - "1.Track.mp3"
/// - "disc1-01.mp3"
///
/// Returns None if no track number can be reliably extracted
pub fn parse_track_number(filename: &str) -> Option<u32> {
    // Remove extension
    let name_without_ext = filename
        .rsplit_once('.')
        .map(|(name, _)| name)
        .unwrap_or(filename);

    // Strategy 1: Look for number at the beginning (most common)
    // Matches: "01 - Track", "01.Track", "01_Track"
    let beginning_pattern = Regex::new(r"^(\d{1,3})[\s\-._]").ok()?;
    if let Some(caps) = beginning_pattern.captures(name_without_ext) {
        if let Some(num_str) = caps.get(1) {
            if let Ok(num) = num_str.as_str().parse::<u32>() {
                // Sanity check: track numbers should be reasonable (1-999)
                if num >= 0 && num < 1000 {
                    return Some(num);
                }
            }
        }
    }

    // Strategy 2: Look for pattern like "#3-A" or "#3"
    let hash_pattern = Regex::new(r"#(\d{1,3})").ok()?;
    if let Some(caps) = hash_pattern.captures(name_without_ext) {
        if let Some(num_str) = caps.get(1) {
            if let Ok(num) = num_str.as_str().parse::<u32>() {
                if num >= 0 && num < 1000 {
                    return Some(num);
                }
            }
        }
    }

    // Strategy 2.5: Look for prefix patterns like "tr01", "tk05", "track03"
    // Common in Japanese audio works
    let prefix_pattern = Regex::new(r"^(?:tr|tk|track|ch|se|bgm)(\d{1,3})[\s\-._]").ok()?;
    if let Some(caps) = prefix_pattern.captures(name_without_ext.to_lowercase().as_str()) {
        if let Some(num_str) = caps.get(1) {
            if let Ok(num) = num_str.as_str().parse::<u32>() {
                if num >= 0 && num < 1000 {
                    return Some(num);
                }
            }
        }
    }

    // Strategy 3: Look for pattern like "disc1-01" or "cd2-05"
    let disc_pattern = Regex::new(r"(?:disc|cd|track)[\s\-._]?(\d{1,3})[\s\-._](\d{1,3})").ok()?;
    if let Some(caps) = disc_pattern.captures(name_without_ext.to_lowercase().as_str()) {
        // Take the second number (track number after disc number)
        if let Some(num_str) = caps.get(2) {
            if let Ok(num) = num_str.as_str().parse::<u32>() {
                if num >= 0 && num < 1000 {
                    return Some(num);
                }
            }
        }
    }

    // Strategy 4: Look for any number with separators (fallback)
    // Matches patterns like "prefix_01_Track" or "Track_01"
    let separator_pattern = Regex::new(r"[\s\-._](\d{1,3})[\s\-._]").ok()?;
    if let Some(caps) = separator_pattern.captures(name_without_ext) {
        if let Some(num_str) = caps.get(1) {
            if let Ok(num) = num_str.as_str().parse::<u32>() {
                if num >= 0 && num < 1000 {
                    return Some(num);
                }
            }
        }
    }

    // Strategy 5: Look for number at the end "Track 01"
    let end_pattern = Regex::new(r"[\s\-._](\d{1,3})$").ok()?;
    if let Some(caps) = end_pattern.captures(name_without_ext) {
        if let Some(num_str) = caps.get(1) {
            if let Ok(num) = num_str.as_str().parse::<u32>() {
                if num >= 0 && num < 1000 {
                    return Some(num);
                }
            }
        }
    }

    // If no pattern matched, return None
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_formats() {
        assert_eq!(parse_track_number("01 - Track.mp3"), Some(1));
        assert_eq!(parse_track_number("01.Track.flac"), Some(1));
        assert_eq!(parse_track_number("01_Track.mp3"), Some(1));
        assert_eq!(parse_track_number("1.Track.mp3"), Some(1));
    }

    #[test]
    fn test_prefix_formats() {
        assert_eq!(parse_track_number("prefix_01_Track.mp3"), Some(1));
        assert_eq!(parse_track_number("RJ123456_05.mp3"), Some(5));
    }

    #[test]
    fn test_hash_format() {
        assert_eq!(parse_track_number("#3-A.titre.mp3"), Some(3));
        assert_eq!(parse_track_number("#10.mp3"), Some(10));
    }

    #[test]
    fn test_disc_format() {
        assert_eq!(parse_track_number("disc1-01.mp3"), Some(1));
        assert_eq!(parse_track_number("CD2-05.flac"), Some(5));
    }

    #[test]
    fn test_end_format() {
        assert_eq!(parse_track_number("Track 01.mp3"), Some(1));
        assert_eq!(parse_track_number("Song 15.flac"), Some(15));
    }

    #[test]
    fn test_no_match() {
        assert_eq!(parse_track_number("NoNumber.mp3"), None);
        assert_eq!(parse_track_number("Track.flac"), None);
    }

    #[test]
    fn test_sanity_bounds() {
        assert_eq!(parse_track_number("0.mp3"), None); // 0 is invalid
        assert_eq!(parse_track_number("1000.mp3"), None); // too large
        assert_eq!(parse_track_number("99.mp3"), Some(99)); // valid
    }
}
