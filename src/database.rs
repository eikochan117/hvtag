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
    conn.execute(&init_table(DB_RELEASE_DATE_NAME, DB_RELEASE_DATE_COLS), [])?;
    conn.execute(&init_table(DB_RATING_NAME, DB_RATING_COLS), [])?;
    conn.execute(&init_table(DB_STARS_NAME, DB_STARS_COLS), [])?;
    conn.execute(&init_table(DB_WORKS_NAME, DB_WORKS_COLS), [])?;
    conn.execute(&init_table(DB_CVS_NAME, DB_CVS_COLS), [])?;
    conn.execute(&init_table(DB_LKP_WORK_CVS_NAME, DB_LKP_WORK_CVS_COLS), [])?;
    conn.execute(&init_table(DB_DLSITE_ERRORS_NAME, DB_DLSITE_ERRORS_COLS), [])?;
    conn.execute(&init_table(DB_DLSITE_COVERS_LINK_NAME, DB_DLSITE_COVERS_LINK_COLS), [])?;
    Ok(())
}
