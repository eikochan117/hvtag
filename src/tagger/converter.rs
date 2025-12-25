use std::path::Path;
use std::process::Command;
use tracing::debug;
use crate::errors::HvtError;

/// Converts an audio file to MP3 using ffmpeg
///
/// # Arguments
/// * `input` - Path to the input audio file
/// * `output` - Path to the output MP3 file
/// * `bitrate` - Target bitrate in kbps (e.g., 320)
///
/// # Returns
/// Ok(()) if conversion succeeds, Err otherwise
///
/// # Note
/// Requires ffmpeg to be installed and available in PATH
pub async fn convert_to_mp3(
    input: &Path,
    output: &Path,
    bitrate: u32,
) -> Result<(), HvtError> {
    let input_str = input.to_str()
        .ok_or_else(|| HvtError::AudioConversion("Invalid input path".to_string()))?;

    let output_str = output.to_str()
        .ok_or_else(|| HvtError::AudioConversion("Invalid output path".to_string()))?;

    let bitrate_str = format!("{}k", bitrate);

    let status = Command::new("ffmpeg")
        .args(&[
            "-i", input_str,
            "-codec:a", "libmp3lame",
            "-b:a", &bitrate_str,
            "-y",  // Overwrite output file if it exists
            output_str,
        ])
        .status()
        .map_err(|e| HvtError::AudioConversion(format!("Failed to execute ffmpeg: {}", e)))?;

    if !status.success() {
        return Err(HvtError::AudioConversion(
            format!("ffmpeg exited with status: {}", status)
        ));
    }

    Ok(())
}

/// Converts an audio file to MP3 in-place (replaces original)
///
/// # Arguments
/// * `file_path` - Path to the audio file to convert
/// * `bitrate` - Target bitrate in kbps (e.g., 320)
///
/// # Returns
/// Ok(()) if conversion succeeds and original is deleted, Err otherwise
///
/// # Note
/// This function:
/// 1. Converts the file to a temporary .mp3
/// 2. Deletes the original file
/// 3. Renames the temporary file to replace the original (with .mp3 extension)
pub async fn convert_to_mp3_in_place(
    file_path: &Path,
    bitrate: u32,
) -> Result<(), HvtError> {
    // Create temporary output path
    let temp_output = file_path.with_extension("mp3.tmp");

    // Convert to temp file
    convert_to_mp3(file_path, &temp_output, bitrate).await?;

    // Delete original
    std::fs::remove_file(file_path)
        .map_err(|e| HvtError::Io(e))?;

    // Rename temp to final (with .mp3 extension)
    let final_path = file_path.with_extension("mp3");
    std::fs::rename(&temp_output, &final_path)
        .map_err(|e| HvtError::Io(e))?;

    debug!("Converted and replaced: {} -> {}", file_path.display(), final_path.display());
    Ok(())
}

/// Checks if ffmpeg is available in the system PATH
pub fn is_ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
