use std::path::{Path, PathBuf};
use std::fs;
use regex::Regex;
use tracing::{info, debug, warn};
use crate::errors::HvtError;

fn rjcode_regex() -> Regex {
    Regex::new(r"((?:RJ|VJ)\d{6,8})").unwrap()
}

/// Scans all direct subdirectories of `source_path` and prepares each for import.
///
/// For each subfolder:
/// - If its name doesn't start with an RJ/VJ code, searches subdirectory names for one and renames
/// - Moves all audio files from any subdirectory to the folder root (flatten)
/// - Removes empty subdirectories
///
/// This must run before `get_list_of_folders` so that the scanner finds correctly-named flat folders.
/// Returns the number of folders that were renamed or had files moved.
pub fn prepare_source_directory(source_path: &str) -> Result<usize, HvtError> {
    let mut count = 0;

    let entries = fs::read_dir(source_path)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match prepare_for_import(&path) {
            Ok(Some(_)) => count += 1,
            Ok(None) => debug!("Skipped (no RJCode found): {}", path.display()),
            Err(e) => warn!(
                "Failed to prepare '{}': {}",
                path.file_name().unwrap_or_default().to_str().unwrap_or("?"),
                e
            ),
        }
    }

    Ok(count)
}

/// Prepares a single source folder for import:
/// 1. If the folder name doesn't start with an RJ/VJ code, searches subdirectory names for one
///    and renames the root folder accordingly
/// 2. Moves all audio files from any subdirectory up to the folder root
/// 3. Removes now-empty subdirectories
///
/// Returns the final folder path, or `None` if no RJCode could be found (folder is skipped).
pub fn prepare_for_import(folder_path: &Path) -> Result<Option<PathBuf>, HvtError> {
    let folder_name = folder_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // --- Step 1: Resolve the canonical RJCode for this folder ---
    let rjcode: String = if folder_name.starts_with("RJ") || folder_name.starts_with("VJ") {
        // Root folder already has the right prefix.
        // Extract just the bare code in case there's trailing text (e.g. "RJ01234567 - Title").
        match rjcode_regex().find(folder_name) {
            Some(m) => m.as_str().to_string(),
            None => folder_name.to_string(), // Shouldn't happen given the starts_with check
        }
    } else {
        // Root is a freeform title — look for the RJCode in subfolder names (max 5 levels deep)
        match find_rjcode_in_subtree(folder_path, 5) {
            Some(code) => code,
            None => {
                debug!("No RJCode found in subtree of '{}'", folder_name);
                return Ok(None);
            }
        }
    };

    // --- Step 2: Rename root folder to the bare RJCode if needed ---
    let final_path: PathBuf = if folder_name != rjcode {
        let parent = folder_path
            .parent()
            .ok_or_else(|| HvtError::PathCreationFailed(folder_path.display().to_string()))?;
        let new_path = parent.join(&rjcode);

        if new_path.exists() {
            warn!(
                "Cannot rename '{}' → '{}': target already exists, skipping folder",
                folder_name, rjcode
            );
            return Ok(None);
        }

        info!("Renaming '{}' → '{}'", folder_name, rjcode);
        fs::rename(folder_path, &new_path)?;
        new_path
    } else {
        folder_path.to_path_buf()
    };

    // --- Step 3: Flatten audio files to root ---
    normalize_folder_structure(&final_path)?;

    Ok(Some(final_path))
}

/// Moves all audio files that are inside subdirectories up to `folder_path` root.
/// Removes empty subdirectories afterwards.
/// Returns the number of files moved (0 if already flat).
pub fn normalize_folder_structure(folder_path: &Path) -> Result<usize, HvtError> {
    let mut files_to_move: Vec<PathBuf> = Vec::new();
    collect_audio_in_subdirs(folder_path, folder_path, &mut files_to_move)?;

    if files_to_move.is_empty() {
        debug!("Already flat: {}", folder_path.display());
        return Ok(0);
    }

    for source in &files_to_move {
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| HvtError::PathCreationFailed(source.display().to_string()))?;

        let dest = resolve_filename_conflict(&folder_path.join(name))?;
        debug!(
            "Moving {} → {}",
            source.display(),
            dest.file_name().unwrap().to_string_lossy()
        );
        fs::rename(source, &dest)?;
    }

    cleanup_empty_subdirs(folder_path)?;

    info!("Normalized: {} file(s) moved to root", files_to_move.len());
    Ok(files_to_move.len())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Walks `current` recursively and appends audio files that are NOT directly
/// under `root` (i.e. files that need to be moved up).
fn collect_audio_in_subdirs(
    current: &Path,
    root: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), HvtError> {
    let entries = fs::read_dir(current)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_audio_in_subdirs(&path, root, out)?;
        } else if path.is_file() && path.parent() != Some(root) {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext.to_lowercase().as_str(), "mp3" | "flac" | "wav" | "ogg") {
                    out.push(path);
                }
            }
        }
    }
    Ok(())
}

/// Searches directory names up to `max_depth` levels deep for an RJ/VJ code.
/// Returns the first code found (breadth-first within each level).
fn find_rjcode_in_subtree(path: &Path, max_depth: u32) -> Option<String> {
    if max_depth == 0 {
        return None;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return None;
    };

    let mut subdirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let child = entry.path();
        if !child.is_dir() {
            continue;
        }

        if let Some(name) = child.file_name().and_then(|n| n.to_str()) {
            if let Some(m) = rjcode_regex().find(name) {
                return Some(m.as_str().to_string());
            }
        }

        subdirs.push(child);
    }

    // Nothing found at this level — recurse into children
    for subdir in subdirs {
        if let Some(code) = find_rjcode_in_subtree(&subdir, max_depth - 1) {
            return Some(code);
        }
    }

    None
}

/// Appends a numeric suffix to resolve a filename collision (e.g. `track_1.mp3`).
fn resolve_filename_conflict(path: &Path) -> Result<PathBuf, HvtError> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| HvtError::PathCreationFailed(path.display().to_string()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let parent = path
        .parent()
        .ok_or_else(|| HvtError::PathCreationFailed(path.display().to_string()))?;

    for i in 1..1000 {
        let candidate = if ext.is_empty() {
            format!("{}_{}", stem, i)
        } else {
            format!("{}_{}.{}", stem, i, ext)
        };
        let candidate_path = parent.join(candidate);
        if !candidate_path.exists() {
            return Ok(candidate_path);
        }
    }

    Err(HvtError::PathCreationFailed(format!(
        "Could not resolve filename conflict for {}",
        path.display()
    )))
}

/// Recursively removes empty subdirectories (depth-first so nested empties are cleaned up).
fn cleanup_empty_subdirs(folder_path: &Path) -> Result<(), HvtError> {
    let entries = fs::read_dir(folder_path)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            cleanup_empty_subdirs(&path)?;
            let _ = fs::remove_dir(&path); // no-op if non-empty
        }
    }
    Ok(())
}
