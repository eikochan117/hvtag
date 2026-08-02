use rusqlite::Connection;

use crate::{config::Config, database::queries, errors::HvtError, folders::types::ManagedFolder};
use std::fs;

pub mod types;

/// Renvoie la liste des dossier dans le path indiqué
pub fn get_list_of_folders(base_path: &str) -> Result<Vec<ManagedFolder>, HvtError> {
    let mut directories = Vec::new();

    let entries = fs::read_dir(base_path)
        .map_err(|_| HvtError::FolderReading(base_path.to_string()))?;

    for entry in entries {
        let entry = entry
            .map_err(|_| HvtError::FolderReading("<unknown>".to_string()))?;
        let path = entry.path();

        if path.is_dir() {
            directories.push(
                ManagedFolder::new(
                    path
                    .to_string_lossy()
                    .to_string()
                )
            );
        }
    }

    let res = directories
        .into_iter()
        .filter(|x| x.is_valid)
        .collect();
    Ok(res)
}

/// Enregistre les dossiers dans la db. Paths are stored in portable `$library`/`$source` form
/// (see `crate::paths`) so the database survives moving to a deployment where those directories
/// are mounted somewhere else.
pub fn register_folders(conn: &Connection, config: &Config, folder_list: Vec<ManagedFolder>) -> Result<(), HvtError> {
    for fld in &folder_list {
        let mut stored = fld.clone();
        stored.path = crate::paths::to_stored_path(config, &fld.path);
        queries::insert_managed_folder(conn, &stored)?;
    }

    Ok(())
}

