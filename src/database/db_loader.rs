use std::{fs, path::PathBuf};

use rusqlite::Connection;

use crate::errors::HvtError;

pub fn get_default_db_path() -> Result<String, HvtError> {
    // Use platform-appropriate data directory
    let data_dir = if cfg!(target_os = "windows") {
        // On Windows, use AppData\Local
        dirs::data_local_dir()
            .ok_or_else(|| HvtError::Generic("Could not determine local data directory".to_string()))?
            .join("hvtag")
    } else {
        // On Linux/macOS, use ~/.hvtag
        dirs::home_dir()
            .ok_or_else(|| HvtError::Generic("Could not determine home directory".to_string()))?
            .join(".hvtag")
    };

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .map_err(|_| HvtError::PathCreationFailed(data_dir.display().to_string()))?;
    }

    let db_path = data_dir.join("data.db3");
    db_path.to_str()
        .ok_or_else(|| HvtError::PathCreationFailed(data_dir.display().to_string()))
        .map(|s| s.to_string())
}

pub fn open_db(custom_path: Option<&str>) -> Result<Connection, HvtError> {
    let path = match custom_path {
        Some(p) => p.to_string(),
        None => get_default_db_path()?
    };
    let conn = Connection::open(path)?;

    // CRITICAL: Enable foreign keys (SQLite disables them by default)
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    Ok(conn)
}
