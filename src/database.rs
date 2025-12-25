use rusqlite::Connection;

use crate::{database::{sql::{init_db, init_table}, tables::*}, errors::HvtError};

pub mod db_loader;
pub mod migration;
pub mod queries;
pub mod sql;
pub mod tables;
pub mod custom_tags;

pub fn init(conn: &Connection) -> Result<(), HvtError> {
    // Ensure foreign keys are enabled (additional safety check)
    conn.execute("PRAGMA foreign_keys = ON", [])?;

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

    // New tables for enhanced tracking and historization
    conn.execute(&init_table(DB_FILE_PROCESSING_NAME, DB_FILE_PROCESSING_COLS), [])?;
    conn.execute(&init_table(DB_PROCESSING_HISTORY_NAME, DB_PROCESSING_HISTORY_COLS), [])?;
    conn.execute(&init_table(DB_METADATA_HISTORY_NAME, DB_METADATA_HISTORY_COLS), [])?;

    // Custom tags table (global mapping)
    conn.execute(&init_table(DB_CUSTOM_TAG_MAPPINGS_NAME, DB_CUSTOM_TAG_MAPPINGS_COLS), [])?;
    conn.execute(DB_FILE_PROCESSING_INDEX_FLD_ID, [])?;
    conn.execute(DB_FILE_PROCESSING_INDEX_TAG_DATE, [])?;

    // Run migrations to add new columns to existing tables
    migration::migrate_schema(conn)?;

    // Run database normalization migration (FK/PK constraints)
    migration::migrate_add_constraints(conn)?;

    Ok(())
}
