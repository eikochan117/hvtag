use std::{error::Error, fmt::Display};

use crate::folders::types::RJCode;

#[derive(Debug)]
pub enum HvtError {
    GenericError(Box<dyn Error>),
    FolderReadingError(String),
    SqliteError(rusqlite::Error),
    RemovedWork(RJCode),
}

impl From<rusqlite::Error> for HvtError {
    fn from(value: rusqlite::Error) -> Self {
        Self::SqliteError(value)
    }
}

impl Error for HvtError {}

impl Display for HvtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HvtError::GenericError(x) => write!(f, "Generic Error : {x}"),
            HvtError::FolderReadingError(x) => write!(f, "Error reading folder : {x}"),
            HvtError::SqliteError(x) => write!(f, "Error SQLite : {x}"),
            HvtError::RemovedWork(x) => write!(f, "Removed work : {x}"),
        }
    }
}

#[derive(Debug)]
pub enum DbLoaderError {
    UnsupportedOS(String),
    PathCreationFailed(String),
    UnavailableEnvVariable(String),
    SqliteError(rusqlite::Error)
}

impl From<rusqlite::Error> for DbLoaderError {
    fn from(value: rusqlite::Error) -> Self {
        Self::SqliteError(value)
    }
}

impl Error for DbLoaderError {}

impl Display for DbLoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbLoaderError::UnsupportedOS(x) => write!(f, "Operating System is not supported : {x}"),
            DbLoaderError::PathCreationFailed(x) => write!(f, "Could not create path : {x}"),
            DbLoaderError::UnavailableEnvVariable(x) => write!(f, "Could not get env value : {x}"),
            DbLoaderError::SqliteError(x) => write!(f, "SQLite error : {x}")
        }
    }
}

#[derive(Debug)]
pub enum DatabaseError {
    SqliteError(rusqlite::Error)
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(value: rusqlite::Error) -> Self {
        Self::SqliteError(value)
    }
}

impl Error for DatabaseError { }

impl Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::SqliteError(x) => write!(f, "SQLite error : {x}")
        }
    }
}
