use rusqlite::Connection;

use crate::{database::{sql::{get_all_works, get_max_id, get_unscanned_works, insert_managed_folder}, tables::DB_FOLDERS_NAME}, errors::HvtError, folders::types::{ManagedFolder, RJCode}};
use std::fs;

pub mod types;

pub fn get_list_of_folders(base_path: &str) -> Result<Vec<ManagedFolder>, HvtError> {
    let mut directories = Vec::new();

    let entries = fs::read_dir(base_path)
        .map_err(|_| HvtError::FolderReadingError(base_path.to_string()))?;

    for entry in entries {
        let entry = entry
            .map_err(|_| HvtError::FolderReadingError("<unknown>".to_string()))?;
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

pub fn register_folders(conn: &Connection, folder_list: Vec<ManagedFolder>) -> Result<(), HvtError> {
    let mut max_fld_id: usize = conn.query_one(&get_max_id("fld_id", DB_FOLDERS_NAME), [], |x| x.get(0))?;
    for fld in &folder_list {
        max_fld_id += conn.execute(&insert_managed_folder(fld, max_fld_id + 1), [])?;
    }

    Ok(())
}

pub fn get_list_of_unscanned_works(conn: &Connection, max_cnt: Option<usize>) -> Result<Vec<RJCode>, HvtError> {
    let mut entries = conn.prepare(&get_unscanned_works())?;
    let vals = entries.query_map([], |x| x.get("rjcode"))?;
    let rjcodes : Vec<RJCode> = vals.map(|x| x.unwrap()).collect();
    if let Some(x) = max_cnt {
        let res = rjcodes.into_iter().take(x).collect();
        Ok(res)
    } else {
        Ok(rjcodes)
    }
}

pub fn get_list_of_all_works(conn: &Connection, max_cnt: Option<usize>) -> Result<Vec<RJCode>, HvtError> {
    let mut entries = conn.prepare(&get_all_works())?;
    let vals = entries.query_map([], |x| x.get("rjcode"))?;
    let rjcodes : Vec<RJCode> = vals.map(|x| x.unwrap()).collect();
    if let Some(x) = max_cnt {
        let res = rjcodes.into_iter().take(x).collect();
        Ok(res)
    } else {
        Ok(rjcodes)
    }
}
