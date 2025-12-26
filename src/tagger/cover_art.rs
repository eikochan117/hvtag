use std::path::Path;
use tracing::{debug, error};
use crate::errors::HvtError;
use image::ImageFormat;

/// Downloads cover art from URL and saves it as folder.jpeg
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
    error!("{url}");
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
