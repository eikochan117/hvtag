use std::{env, error::Error, fmt::Display, fs, path::{self, Path}};

use duckdb::{Connection};

#[derive(Debug)]
pub enum DbLoaderError {
    UnsupportedOS(String),
    PathCreationFailed(String),
    UnavailableEnvVariable(String),
    DuckDbError(duckdb::Error)
}

impl From<duckdb::Error> for DbLoaderError {
    fn from(value: duckdb::Error) -> Self {
        Self::DuckDbError(value)
    }
}

impl Error for DbLoaderError {}

impl Display for DbLoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbLoaderError::UnsupportedOS(x) => write!(f, "Operating System is not supported : {x}"),
            DbLoaderError::PathCreationFailed(x) => write!(f, "Could not create path : {x}"),
            DbLoaderError::UnavailableEnvVariable(x) => write!(f, "Could not get env value : {x}"),
            DbLoaderError::DuckDbError(x) => write!(f, "DuckDb error : {x}")
        }
    }
}


pub fn get_default_db_path() -> Result<String, DbLoaderError> {
    let os = std::env::consts::OS;
    let v = match os {
        "windows" => String::from("USERNAME"),
        "linux" => String::from("USER"),
        x => return Err(DbLoaderError::UnsupportedOS(x.to_string()))
    };

    let username = match env::var(&v) {
        Ok(x) => x,
        Err(_) => return Err(DbLoaderError::UnavailableEnvVariable(v))
    };
    let path_f = match os {
        "windows" => format!("C:\\Users\\{username}\\AppData\\Local\\hvtag"),
        "linux" => format!("/home/{username}/.hvtag"),
        x => return Err(DbLoaderError::UnsupportedOS(x.to_string()))
    };

    let path = Path::new(&path_f);
    if !path.exists() {
        if let Err(_) = fs::create_dir_all(path) {
            return Err(DbLoaderError::PathCreationFailed(path_f));
        }
    }

    Ok(path.to_str().map(|x| format!("{x}\\data.ddb")).unwrap())
}

pub fn open_db(custom_path: Option<&str>) -> Result<Connection, DbLoaderError> {
    let conn = Connection::open(custom_path.unwrap_or(&get_default_db_path()?))?;
    Ok(conn)
}
