use rusqlite::Connection;
use crate::errors::HvtError;

/// Migrates the database schema to add new columns to existing tables
/// This function is idempotent and can be called multiple times safely
pub fn migrate_schema(conn: &Connection) -> Result<(), HvtError> {
    migrate_folders_table(conn)?;
    migrate_dlsite_errors_table(conn)?;
    Ok(())
}

/// Adds processing tracking columns to the folders table
fn migrate_folders_table(conn: &Connection) -> Result<(), HvtError> {
    // Check if migration is needed by trying to select a new column
    let needs_migration = conn
        .prepare("SELECT processing_status FROM folders LIMIT 1")
        .is_err();

    if needs_migration {
        // Add new columns for processing status tracking
        conn.execute(
            "ALTER TABLE folders ADD COLUMN processing_status TEXT DEFAULT 'pending'",
            [],
        )?;
        conn.execute(
            "ALTER TABLE folders ADD COLUMN completion_percentage INTEGER DEFAULT 0",
            [],
        )?;
        conn.execute(
            "ALTER TABLE folders ADD COLUMN total_files_to_process INTEGER",
            [],
        )?;
        conn.execute(
            "ALTER TABLE folders ADD COLUMN files_processed INTEGER DEFAULT 0",
            [],
        )?;
        conn.execute(
            "ALTER TABLE folders ADD COLUMN started_processing TIMESTAMP",
            [],
        )?;
        conn.execute(
            "ALTER TABLE folders ADD COLUMN finished_processing TIMESTAMP",
            [],
        )?;
    }

    Ok(())
}

/// Adds error tracking columns to the dlsite_errors table
fn migrate_dlsite_errors_table(conn: &Connection) -> Result<(), HvtError> {
    // Check if migration is needed
    let needs_migration = conn
        .prepare("SELECT error_timestamp FROM dlsite_errors LIMIT 1")
        .is_err();

    if needs_migration {
        // Add new columns for enhanced error tracking
        conn.execute(
            "ALTER TABLE dlsite_errors ADD COLUMN error_timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
            [],
        )?;
        conn.execute(
            "ALTER TABLE dlsite_errors ADD COLUMN retry_count INTEGER DEFAULT 0",
            [],
        )?;
        conn.execute(
            "ALTER TABLE dlsite_errors ADD COLUMN error_category TEXT",
            [],
        )?;
        conn.execute(
            "ALTER TABLE dlsite_errors ADD COLUMN error_details TEXT",
            [],
        )?;
        conn.execute(
            "ALTER TABLE dlsite_errors ADD COLUMN is_resolved BOOLEAN DEFAULT 0",
            [],
        )?;
        conn.execute(
            "ALTER TABLE dlsite_errors ADD COLUMN resolved_date TIMESTAMP",
            [],
        )?;
    }

    Ok(())
}

/// Placeholder for future database migrations
/// Currently not needed as the database can be reset at will during development
///
/// When the application is production-ready, add migration functions here
/// to handle schema changes for existing databases
pub fn migrate_add_constraints(_conn: &Connection) -> Result<(), HvtError> {
    // TODO: Add future migrations here when needed
    // Example:
    // if needs_migration_v2() {
    //     run_migration_v2(conn)?;
    // }

    Ok(())
}
