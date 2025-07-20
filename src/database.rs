use rusqlite::Connection;

use crate::{database::{sql::{init_db, init_table}, tables::*}, errors::DatabaseError};

pub mod db_loader;
pub mod sql;
pub mod tables;

pub fn init(conn: &Connection) -> Result<(), DatabaseError> {
    conn.execute(&init_db(), [])?;
    conn.execute(&init_table(DB_FOLDERS_NAME, DB_FOLDERS_COLS), [])?;
    conn.execute(&init_table(DB_DLSITE_SCAN_NAME, DB_DLSITE_SCAN_COLS), [])?;
    conn.execute(&init_table(DB_DLSITE_TAG_NAME, DB_DLSITE_TAG_COLS), [])?;
    conn.execute(&init_table(DB_CIRCLE_NAME, DB_CIRCLE_COLS), [])?;
    conn.execute(&init_table(DB_LKP_WORK_CIRCLE_NAME, DB_LKP_WORK_CIRCLE_COLS), [])?;
    conn.execute(&init_table(DB_LKP_WORK_TAG_NAME, DB_LKP_WORK_TAG_COLS), [])?;
    Ok(())
}
