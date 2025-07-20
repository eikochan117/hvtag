use crate::{database::sql::get_unscanned_works, errors::HvtError, folders::types::{ManagedFolder, RJCode}};
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

pub fn get_list_of_unscanned_works(conn: &rusqlite::Connection, max_cnt: Option<usize>) -> Result<Vec<RJCode>, HvtError> {
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
