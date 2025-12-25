use std::{env, fs, path::Path};

use rusqlite::Connection;

use crate::errors::HvtError;

pub fn get_default_db_path() -> Result<String, HvtError> {
    let os = std::env::consts::OS;
    let v = match os {
        "windows" => String::from("USERNAME"),
        "linux" => String::from("USER"),
        x => return Err(HvtError::UnsupportedOS(x.to_string()))
    };

    let username = match env::var(&v) {
        Ok(x) => x,
        Err(_) => return Err(HvtError::UnavailableEnvVariable(v))
    };
    let path_f = match os {
        "windows" => format!("C:\\Users\\{username}\\AppData\\Local\\hvtag"),
        "linux" => format!("/home/{username}/.hvtag"),
        x => return Err(HvtError::UnsupportedOS(x.to_string()))
    };

    let path = Path::new(&path_f);
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|_| HvtError::PathCreationFailed(path_f.clone()))?;
    }

    let db_path = path.to_str()
        .ok_or_else(|| HvtError::PathCreationFailed(path_f.clone()))?
        .to_string();
    Ok(format!("{db_path}/data.db3"))
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
