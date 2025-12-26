use std::path::{Path, PathBuf};
use tracing::debug;
use crate::errors::HvtError;
use image::ImageFormat;

/// Get the cache directory for covers
fn get_cache_dir() -> Result<PathBuf, HvtError> {
    let home = std::env::var("HOME")
        .map_err(|_| HvtError::Generic("HOME environment variable not set".to_string()))?;

    let cache_dir = PathBuf::from(home).join(".hvtag").join("covers_cache");

    // Create cache directory if it doesn't exist
    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| HvtError::Generic(format!("Failed to create cache directory: {}", e)))?;
    }

    Ok(cache_dir)
}

/// Downloads cover art from URL and saves it to local cache
///
/// # Arguments
/// * `url` - The URL of the image to download
/// * `rjcode` - The RJ code of the work (used as cache filename)
/// * `target_size` - Optional target size (width, height) for resizing. If None, keeps original size.
///
/// # Returns
/// Ok(PathBuf) with path to cached cover, Err if download or save fails
pub async fn download_cover_to_cache(
    url: &str,
    rjcode: &str,
    target_size: Option<(u32, u32)>,
) -> Result<PathBuf, HvtError> {
    // Download image from URL
    let response = reqwest::get(url)
        .await
        .map_err(|e| HvtError::Http(format!("Failed to download cover art: {}", e)))?;

    if !response.status().is_success() {
        return Err(HvtError::Http(format!(
            "HTTP {} when downloading cover art",
            response.status()
        )));
    }

    let bytes = response.bytes()
        .await
        .map_err(|e| HvtError::Http(format!("Failed to read cover art bytes: {}", e)))?;

    // Load image
    let img = image::load_from_memory(&bytes)
        .map_err(|e| HvtError::Image(format!("Failed to decode image: {}", e)))?;

    // Optionally resize
    let final_img = if let Some((width, height)) = target_size {
        img.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    // Save to cache with RJCode as filename
    let cache_dir = get_cache_dir()?;
    let cache_path = cache_dir.join(format!("{}.jpeg", rjcode));

    final_img.save_with_format(&cache_path, ImageFormat::Jpeg)
        .map_err(|e| HvtError::Image(format!("Failed to save cover to cache: {}", e)))?;

    debug!("Cover cached at: {}", cache_path.display());
    Ok(cache_path)
}

/// Copy cover from cache to final folder location
///
/// # Arguments
/// * `rjcode` - The RJ code of the work
/// * `folder_path` - The destination folder path
///
/// # Returns
/// Ok(()) if successful, Err if copy fails
pub fn copy_cover_from_cache(
    rjcode: &str,
    folder_path: &Path,
) -> Result<(), HvtError> {
    let cache_dir = get_cache_dir()?;
    let cache_path = cache_dir.join(format!("{}.jpeg", rjcode));

    if !cache_path.exists() {
        return Err(HvtError::Generic(format!(
            "Cached cover not found for {}: {}",
            rjcode,
            cache_path.display()
        )));
    }

    let dest_path = folder_path.join("folder.jpeg");

    std::fs::copy(&cache_path, &dest_path)
        .map_err(|e| HvtError::Generic(format!("Failed to copy cover from cache: {}", e)))?;

    debug!("Cover copied from cache to: {}", dest_path.display());

    // Clean up cache after successful copy
    let _ = std::fs::remove_file(&cache_path);

    Ok(())
}

/// Downloads cover art from URL and saves it as folder.jpeg (LEGACY - direct save)
///
/// # Arguments
/// * `url` - The URL of the image to download
/// * `folder_path` - The path to the folder where folder.jpeg will be saved
/// * `target_size` - Optional target size (width, height) for resizing. If None, keeps original size.
///
/// # Returns
/// Ok(()) if successful, Err if download or save fails
pub async fn download_and_save_cover(
    url: &str,
    folder_path: &Path,
    target_size: Option<(u32, u32)>,
) -> Result<(), HvtError> {
    // Download image from URL
    debug!("Downloading cover from: {}", url);
    let response = reqwest::get(url)
        .await
        .map_err(|e| HvtError::Http(format!("Failed to download cover art: {}", e)))?;

    if !response.status().is_success() {
        return Err(HvtError::Http(format!(
            "HTTP {} when downloading cover art",
            response.status()
        )));
    }

    let bytes = response.bytes()
        .await
        .map_err(|e| HvtError::Http(format!("Failed to read cover art bytes: {}", e)))?;

    // Load image
    let img = image::load_from_memory(&bytes)
        .map_err(|e| HvtError::Image(format!("Failed to decode image: {}", e)))?;

    // Optionally resize
    let final_img = if let Some((width, height)) = target_size {
        img.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    // Save to folder.jpeg
    let cover_path = folder_path.join("folder.jpeg");
    final_img.save_with_format(&cover_path, ImageFormat::Jpeg)
        .map_err(|e| HvtError::Image(format!("Failed to save cover art: {}", e)))?;

    debug!("Cover art saved to: {}", cover_path.display());
    Ok(())
}

/// Checks if folder.jpeg already exists in the given folder
pub fn has_cover_art(folder_path: &Path) -> bool {
    folder_path.join("folder.jpeg").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_has_cover_art() {
        let path = PathBuf::from("/tmp/test_folder");
        // This will return false if the folder doesn't exist or no folder.jpeg
        assert_eq!(has_cover_art(&path), false);
    }
}
