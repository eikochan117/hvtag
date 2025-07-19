use std::{error::Error, fmt::Display};

pub enum HvtError {
    GenericError(Box<dyn Error>)
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
