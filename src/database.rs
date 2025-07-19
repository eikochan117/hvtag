use duckdb::Connection;

use crate::{database::{sql::init_table, tables::*}, errors::DatabaseError};

pub mod db_loader;
pub mod sql;
pub mod tables;

pub fn init(conn: &Connection) -> Result<(), DatabaseError> {
    conn.execute(&init_table(DB_FOLDERS_NAME, DB_FOLDERS_COLS), [])?;
    Ok(())
}
