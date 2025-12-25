use std::path::{Path, PathBuf};
use std::fs;
use tracing::{info, debug};
use crate::errors::HvtError;

/// Represents different folder architecture patterns found in audio works
#[derive(Debug, Clone, PartialEq)]
pub enum FolderPattern {
    /// All audio files are directly in the RJ folder (already normalized)
    /// Example: RJ123456/track01.mp3
    Flat,

    /// Audio files are in a subdirectory named "mp3"
    /// Example: RJ123456/mp3/track01.mp3
    Mp3Subfolder,

    /// Audio files are in a subdirectory named "audio"
    /// Example: RJ123456/audio/track01.mp3
    AudioSubfolder,

    /// Audio files are in a subdirectory named "wav" or "flac"
    /// Example: RJ123456/wav/track01.wav
    FormatSubfolder,

    /// Audio files are in numbered subdirectories (disc1, disc2, etc.)
    /// Example: RJ123456/disc1/track01.mp3
    DiscSubfolders,

    /// Audio files are in language-specific subdirectories (jp, en, etc.)
    /// Example: RJ123456/jp/track01.mp3
    LanguageSubfolders,

    /// Mixed structure with multiple patterns
    Mixed,
}

/// Strategy for normalizing a specific folder pattern
#[derive(Debug)]
struct NormalizationStrategy {
    pattern: FolderPattern,
    /// Subdirectory names to scan (relative to RJ folder)
    subdirs_to_check: Vec<String>,
    /// Whether to preserve subdirectory name in filename when moving
    preserve_subdir_in_name: bool,
}

impl NormalizationStrategy {
    fn new(pattern: FolderPattern) -> Self {
        match pattern {
            FolderPattern::Flat => NormalizationStrategy {
                pattern,
                subdirs_to_check: vec![],
                preserve_subdir_in_name: false,
            },
            FolderPattern::Mp3Subfolder => NormalizationStrategy {
                pattern,
                subdirs_to_check: vec!["mp3".to_string()],
                preserve_subdir_in_name: false,
            },
            FolderPattern::AudioSubfolder => NormalizationStrategy {
                pattern,
                subdirs_to_check: vec!["audio".to_string()],
                preserve_subdir_in_name: false,
            },
            FolderPattern::FormatSubfolder => NormalizationStrategy {
                pattern,
                subdirs_to_check: vec!["wav".to_string(), "flac".to_string(), "ogg".to_string()],
                preserve_subdir_in_name: false,
            },
            FolderPattern::DiscSubfolders => NormalizationStrategy {
                pattern,
                // Will be detected dynamically (disc1, disc2, etc.)
                subdirs_to_check: vec![],
                preserve_subdir_in_name: true, // Keep "disc1_" prefix
            },
            FolderPattern::LanguageSubfolders => NormalizationStrategy {
                pattern,
                subdirs_to_check: vec!["jp".to_string(), "en".to_string(), "cn".to_string()],
                preserve_subdir_in_name: true, // Keep "jp_" prefix
            },
            FolderPattern::Mixed => NormalizationStrategy {
                pattern,
                subdirs_to_check: vec![],
                preserve_subdir_in_name: false,
            },
        }
    }
}

/// Detects the folder architecture pattern used in a given directory
pub fn detect_folder_pattern(folder_path: &Path) -> Result<FolderPattern, HvtError> {
    let mut has_audio_in_root = false;
    let mut has_mp3_subdir = false;
    let mut has_audio_subdir = false;
    let mut has_format_subdir = false;
    let mut has_disc_subdirs = false;
    let mut has_lang_subdirs = false;

    // Scan immediate directory
    let entries = fs::read_dir(folder_path)?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if matches!(ext.to_str().unwrap_or(""), "mp3" | "flac" | "wav" | "ogg") {
                    has_audio_in_root = true;
                }
            }
        } else if path.is_dir() {
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                let dir_name_lower = dir_name.to_lowercase();

                // Check for specific subdirectory patterns
                if dir_name_lower == "mp3" {
                    has_mp3_subdir = has_audio_files_in_dir(&path)?;
                } else if dir_name_lower == "audio" {
                    has_audio_subdir = has_audio_files_in_dir(&path)?;
                } else if matches!(dir_name_lower.as_str(), "wav" | "flac" | "ogg") {
                    has_format_subdir = has_audio_files_in_dir(&path)?;
                } else if dir_name_lower.starts_with("disc") || dir_name_lower.starts_with("cd") {
                    has_disc_subdirs = has_audio_files_in_dir(&path)?;
                } else if matches!(dir_name_lower.as_str(), "jp" | "en" | "cn" | "kr") {
                    has_lang_subdirs = has_audio_files_in_dir(&path)?;
                }
            }
        }
    }

    // Determine pattern based on what was found
    let pattern_count = [has_mp3_subdir, has_audio_subdir, has_format_subdir,
                        has_disc_subdirs, has_lang_subdirs].iter()
                        .filter(|&&x| x).count();

    if pattern_count > 1 {
        return Ok(FolderPattern::Mixed);
    }

    if has_audio_in_root && pattern_count == 0 {
        return Ok(FolderPattern::Flat);
    }

    if has_mp3_subdir {
        Ok(FolderPattern::Mp3Subfolder)
    } else if has_audio_subdir {
        Ok(FolderPattern::AudioSubfolder)
    } else if has_format_subdir {
        Ok(FolderPattern::FormatSubfolder)
    } else if has_disc_subdirs {
        Ok(FolderPattern::DiscSubfolders)
    } else if has_lang_subdirs {
        Ok(FolderPattern::LanguageSubfolders)
    } else {
        Ok(FolderPattern::Flat)
    }
}

/// Checks if a directory contains audio files
fn has_audio_files_in_dir(dir_path: &Path) -> Result<bool, HvtError> {
    let entries = fs::read_dir(dir_path)?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if matches!(ext.to_str().unwrap_or(""), "mp3" | "flac" | "wav" | "ogg") {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

/// Normalizes the folder structure by moving all audio files to the root level
/// Returns the number of files moved
pub fn normalize_folder_structure(folder_path: &Path) -> Result<usize, HvtError> {
    let pattern = detect_folder_pattern(folder_path)?;

    debug!("Detected folder pattern: {:?}", pattern);

    if pattern == FolderPattern::Flat {
        debug!("Folder already normalized, skipping");
        return Ok(0);
    }

    let mut files_moved = 0;

    // Collect all audio files from subdirectories
    let audio_files = collect_audio_files_recursive(folder_path)?;

    for (source_path, relative_subdir) in audio_files {
        // Skip files already in root
        if relative_subdir.is_empty() {
            continue;
        }

        // Generate new filename
        let original_name = source_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| HvtError::PathCreationFailed(source_path.display().to_string()))?;

        let new_filename = if should_preserve_subdir_prefix(&pattern, &relative_subdir) {
            // Preserve subdirectory name as prefix (e.g., "disc1_track01.mp3")
            format!("{}_{}", relative_subdir.replace("/", "_"), original_name)
        } else {
            // Use original filename
            original_name.to_string()
        };

        let dest_path = folder_path.join(&new_filename);

        // Handle filename conflicts
        let final_dest = resolve_filename_conflict(&dest_path)?;

        debug!("Moving: {} -> {}",
               source_path.display(),
               final_dest.file_name().unwrap().to_str().unwrap());

        fs::rename(&source_path, &final_dest)?;
        files_moved += 1;
    }

    // Clean up empty subdirectories
    cleanup_empty_subdirs(folder_path)?;

    info!("Normalized folder structure: {} files moved", files_moved);
    Ok(files_moved)
}

/// Collects all audio files recursively with their relative subdirectory paths
fn collect_audio_files_recursive(folder_path: &Path) -> Result<Vec<(PathBuf, String)>, HvtError> {
    let mut audio_files = Vec::new();
    collect_audio_files_recursive_impl(folder_path, folder_path, &mut audio_files)?;
    Ok(audio_files)
}

fn collect_audio_files_recursive_impl(
    current_path: &Path,
    root_path: &Path,
    audio_files: &mut Vec<(PathBuf, String)>,
) -> Result<(), HvtError> {
    let entries = fs::read_dir(current_path)?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            collect_audio_files_recursive_impl(&path, root_path, audio_files)?;
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                if matches!(ext.to_str().unwrap_or(""), "mp3" | "flac" | "wav" | "ogg") {
                    // Calculate relative subdirectory path
                    let relative_dir = if let Ok(parent) = path.parent()
                        .unwrap_or(current_path)
                        .strip_prefix(root_path) {
                        parent.to_str().unwrap_or("").to_string()
                    } else {
                        String::new()
                    };

                    audio_files.push((path.clone(), relative_dir));
                }
            }
        }
    }

    Ok(())
}

/// Determines if subdirectory name should be preserved as filename prefix
fn should_preserve_subdir_prefix(pattern: &FolderPattern, subdir: &str) -> bool {
    match pattern {
        FolderPattern::DiscSubfolders => true,
        FolderPattern::LanguageSubfolders => true,
        FolderPattern::Mixed => {
            // For mixed patterns, preserve if it looks like disc or language
            let subdir_lower = subdir.to_lowercase();
            subdir_lower.starts_with("disc") ||
            subdir_lower.starts_with("cd") ||
            matches!(subdir_lower.as_str(), "jp" | "en" | "cn" | "kr")
        }
        _ => false,
    }
}

/// Resolves filename conflicts by adding a number suffix
fn resolve_filename_conflict(path: &Path) -> Result<PathBuf, HvtError> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }

    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| HvtError::PathCreationFailed(path.display().to_string()))?;

    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let parent = path.parent()
        .ok_or_else(|| HvtError::PathCreationFailed(path.display().to_string()))?;

    for i in 1..1000 {
        let new_name = if extension.is_empty() {
            format!("{}_{}", stem, i)
        } else {
            format!("{}_{}.{}", stem, i, extension)
        };

        let new_path = parent.join(new_name);
        if !new_path.exists() {
            return Ok(new_path);
        }
    }

    Err(HvtError::PathCreationFailed(
        format!("Could not resolve filename conflict for {}", path.display())
    ))
}

/// Removes empty subdirectories after normalization
fn cleanup_empty_subdirs(folder_path: &Path) -> Result<(), HvtError> {
    let entries = fs::read_dir(folder_path)?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Try to remove directory (will only succeed if empty)
            let _ = fs::remove_dir(&path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_detection() {
        // Tests would go here
    }
}
