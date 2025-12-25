use std::{fmt::Display, fs::{read_dir, DirEntry}, path::Path};
use tracing::{warn, error};
use crate::errors::HvtError;

// Newtype pattern for RJCode with validation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RJCode(String);

impl RJCode {
    pub fn new(s: String) -> Result<Self, HvtError> {
        if s.starts_with("RJ") && s.len() >= 6 {
            Ok(RJCode(s))
        } else {
            Err(HvtError::Parse(format!("Invalid RJCode format: {}", s)))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    // Internal constructor without validation (for when RJ code already validated)
    pub(crate) fn from_string_unchecked(s: String) -> Self {
        RJCode(s)
    }
}

impl Display for RJCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl rusqlite::types::ToSql for RJCode {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.0.as_str()))
    }
}

impl rusqlite::types::FromSql for RJCode {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_str().map(|s| RJCode(s.to_string()))
    }
}

// Newtype pattern for RGCode (circle/maker code)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct RGCode(String);

impl RGCode {
    pub fn new(s: String) -> Self {
        RGCode(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RGCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl rusqlite::types::ToSql for RGCode {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.0.as_str()))
    }
}

impl rusqlite::types::FromSql for RGCode {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_str().map(|s| RGCode(s.to_string()))
    }
}

#[derive(Debug)]
pub struct ManagedFile {
    filename: String,
    extension: String,
    path: String,
}

impl ManagedFile {
    pub fn from_direntry(e: DirEntry) -> Result<Self, HvtError> {
        let filename = e.file_name()
            .into_string()
            .map_err(|_| HvtError::Parse("Invalid filename encoding".to_string()))?;

        let extension = filename
            .split('.')
            .last()
            .unwrap_or("unknown")
            .to_string();

        Ok(ManagedFile {
            filename,
            extension,
            path: e.path().display().to_string()
        })
    }
}

#[derive(Debug)]
pub struct ManagedFolder {
    pub is_valid: bool,
    pub is_tagged: bool,
    pub has_cover: bool,
    pub rjcode: RJCode,
    pub path: String,
    pub files: Vec<ManagedFile>,
}

impl ManagedFolder {
    pub fn new(path: String) -> Self {
        let p = Path::new(&path);
        let mut files = vec![];
        let mut has_audio_files = false;

        // Scan immediate directory for files
        match read_dir(p) {
            Ok(entries) => {
                for e in entries {
                    if let Ok(en) = e {
                        let entry_path = en.path();
                        if entry_path.is_file() {
                            match ManagedFile::from_direntry(en) {
                                Ok(file) => {
                                    // Check if it's an audio file
                                    if matches!(file.extension.as_str(), "mp3" | "flac" | "wav" | "ogg") {
                                        has_audio_files = true;
                                    }
                                    files.push(file);
                                }
                                Err(e) => warn!("Could not process file: {}", e),
                            }
                        } else if entry_path.is_dir() {
                            // Check subdirectories for audio files
                            if let Ok(sub_entries) = read_dir(&entry_path) {
                                for sub_e in sub_entries.flatten() {
                                    if sub_e.path().is_file() {
                                        if let Some(ext) = sub_e.path().extension() {
                                            if matches!(ext.to_str().unwrap_or(""), "mp3" | "flac" | "wav" | "ogg") {
                                                has_audio_files = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error reading directory {}: {}", p.display(), e);
                // Return invalid folder instead of panicking
                return ManagedFolder {
                    is_valid: false,
                    path: path.clone(),
                    files: vec![],
                    is_tagged: false,
                    has_cover: false,
                    rjcode: RJCode::from_string_unchecked(String::new()),
                };
            }
        };

        let is_tagged = files.iter().any(|x| x.extension == "tagged");
        let has_cover = files.iter().any(|x| x.filename == "folder.jpeg");

        let rjcode_str = p.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| String::from(""));

        // Folder is valid if it has RJ prefix and contains audio files (even in subdirectories)
        let is_valid = has_audio_files && rjcode_str.starts_with("RJ");

        ManagedFolder {
            is_valid,
            path: path.to_string(),
            files,
            is_tagged,
            has_cover,
            rjcode: RJCode::from_string_unchecked(rjcode_str),
        }
    }
}
