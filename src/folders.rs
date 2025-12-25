use rusqlite::Connection;

use crate::{database::{queries, tables::DB_FOLDERS_NAME}, errors::HvtError, folders::types::{ManagedFolder, RJCode}};
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

/// Enregistre les dossiers dans la db
pub fn register_folders(conn: &Connection, folder_list: Vec<ManagedFolder>) -> Result<(), HvtError> {
    for fld in &folder_list {
        queries::insert_managed_folder(conn, fld)?;
    }

    Ok(())
}

pub fn get_list_of_unscanned_works(conn: &Connection, max_cnt: Option<usize>) -> Result<Vec<RJCode>, HvtError> {
    let rjcodes = queries::get_unscanned_works(conn)?;

    if let Some(x) = max_cnt {
        let res = rjcodes.into_iter().take(x).collect();
        Ok(res)
    } else {
        Ok(rjcodes)
    }
}

pub fn get_list_of_all_works(conn: &Connection, max_cnt: Option<usize>) -> Result<Vec<RJCode>, HvtError> {
    let rjcodes = queries::get_all_works(conn)?;

    if let Some(x) = max_cnt {
        let res = rjcodes.into_iter().take(x).collect();
        Ok(res)
    } else {
        Ok(rjcodes)
    }
}
