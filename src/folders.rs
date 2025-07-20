use crate::{errors::HvtError, folders::types::ManagedFolder};
use std::fs;

pub mod types;

pub fn get_list_of_folders(base_path: &str) -> Result<Vec<ManagedFolder>, HvtError> {
    let mut directories = Vec::new();

    // Lire le contenu du répertoire
    let entries = fs::read_dir(base_path)
        .map_err(|_| HvtError::FolderReadingError(base_path.to_string()))?;

    for entry in entries {
        let entry = entry
            .map_err(|_| HvtError::FolderReadingError("<unknown>".to_string()))?;
        let path = entry.path();

        // Vérifier si c'est un répertoire
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
