use rusqlite::{Connection, params};
use crate::errors::HvtError;
use crate::folders::types::RJCode;
use crate::database::tables::*;

/// List all DLSite tags used in the database (alphabetically sorted)
/// Returns Vec<(tag_id, tag_name, custom_name_if_mapped, is_ignored)>
pub fn list_all_dlsite_tags(conn: &Connection) -> Result<Vec<(i64, String, Option<String>, bool)>, HvtError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT dt.tag_id, dt.tag_name, ctm.custom_tag_name, COALESCE(ctm.is_ignored, 0)
             FROM {DB_DLSITE_TAG_NAME} dt
             LEFT JOIN {DB_CUSTOM_TAG_MAPPINGS_NAME} ctm ON dt.tag_id = ctm.dlsite_tag_id
             ORDER BY dt.tag_name ASC"
        )
    )?;

    let tags: Vec<(i64, String, Option<String>, bool)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(tags)
}

/// Add or update a global custom tag mapping (rename)
/// This applies to ALL works that have this DLSite tag
pub fn add_custom_tag_mapping(
    conn: &Connection,
    dlsite_tag_name: &str,
    custom_tag_name: &str,
) -> Result<(), HvtError> {
    // Get the tag_id for this DLSite tag
    let tag_id: i64 = conn.query_row(
        &format!("SELECT tag_id FROM {DB_DLSITE_TAG_NAME} WHERE tag_name = ?1"),
        params![dlsite_tag_name],
        |row| row.get(0),
    )?;

    // Insert or replace the mapping (is_ignored = 0 for rename)
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {DB_CUSTOM_TAG_MAPPINGS_NAME}
             (dlsite_tag_id, custom_tag_name, is_ignored, modified_at)
             VALUES (?1, ?2, 0, datetime('now'))"
        ),
        params![tag_id, custom_tag_name],
    )?;

    Ok(())
}

/// Mark a tag as ignored (will not appear in audio file tags)
/// This applies to ALL works that have this DLSite tag
pub fn ignore_tag(
    conn: &Connection,
    dlsite_tag_name: &str,
) -> Result<(), HvtError> {
    // Get the tag_id for this DLSite tag
    let tag_id: i64 = conn.query_row(
        &format!("SELECT tag_id FROM {DB_DLSITE_TAG_NAME} WHERE tag_name = ?1"),
        params![dlsite_tag_name],
        |row| row.get(0),
    )?;

    // Insert or replace the mapping (is_ignored = 1, custom_tag_name = NULL)
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {DB_CUSTOM_TAG_MAPPINGS_NAME}
             (dlsite_tag_id, custom_tag_name, is_ignored, modified_at)
             VALUES (?1, NULL, 1, datetime('now'))"
        ),
        params![tag_id],
    )?;

    Ok(())
}

/// Remove a custom tag mapping (revert to DLSite tag name)
pub fn remove_custom_tag_mapping(
    conn: &Connection,
    dlsite_tag_name: &str,
) -> Result<(), HvtError> {
    // Get the tag_id for this DLSite tag
    let tag_id: i64 = conn.query_row(
        &format!("SELECT tag_id FROM {DB_DLSITE_TAG_NAME} WHERE tag_name = ?1"),
        params![dlsite_tag_name],
        |row| row.get(0),
    )?;

    conn.execute(
        &format!("DELETE FROM {DB_CUSTOM_TAG_MAPPINGS_NAME} WHERE dlsite_tag_id = ?1"),
        params![tag_id],
    )?;

    Ok(())
}

/// Get all custom tag mappings (both renames and ignores)
/// Returns Vec<(dlsite_tag_name, custom_tag_name, is_ignored)>
pub fn get_all_custom_mappings(conn: &Connection) -> Result<Vec<(String, Option<String>, bool)>, HvtError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT dt.tag_name, ctm.custom_tag_name, ctm.is_ignored
             FROM {DB_CUSTOM_TAG_MAPPINGS_NAME} ctm
             JOIN {DB_DLSITE_TAG_NAME} dt ON ctm.dlsite_tag_id = dt.tag_id"
        )
    )?;

    let mappings: Vec<(String, Option<String>, bool)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get::<_, i64>(2)? != 0,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(mappings)
}

/// Get all DLSite tags for a work (without custom mappings applied)
pub fn get_dlsite_tags_for_work(
    conn: &Connection,
    work: &RJCode,
) -> Result<Vec<String>, HvtError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT tag_name FROM {DB_DLSITE_TAG_NAME} WHERE tag_id IN (
                SELECT tag_id FROM {DB_LKP_WORK_TAG_NAME} WHERE fld_id = (
                    SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1
                )
            )"
        )
    )?;

    let tags: Vec<String> = stmt
        .query_map(params![work.as_str()], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(tags)
}

/// Get merged tags for a work (DLSite tags with global custom mappings applied)
/// Filters out tags marked as ignored
pub fn get_merged_tags_for_work(
    conn: &Connection,
    work: &RJCode,
) -> Result<Vec<String>, HvtError> {
    // Get all tags with their custom mappings if they exist
    // Filter out tags where is_ignored = 1
    let mut stmt = conn.prepare(
        &format!(
            "SELECT COALESCE(ctm.custom_tag_name, dt.tag_name) as final_tag_name
             FROM {DB_DLSITE_TAG_NAME} dt
             LEFT JOIN {DB_CUSTOM_TAG_MAPPINGS_NAME} ctm ON dt.tag_id = ctm.dlsite_tag_id
             WHERE dt.tag_id IN (
                 SELECT tag_id FROM {DB_LKP_WORK_TAG_NAME} WHERE fld_id = (
                     SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1
                 )
             )
             AND COALESCE(ctm.is_ignored, 0) = 0"
        )
    )?;

    let mut tags: Vec<String> = stmt
        .query_map(params![work.as_str()], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Deduplicate tags in case multiple DLSite tags are renamed to the same custom name
    tags.sort();
    tags.dedup();

    Ok(tags)
}

/// Get the modification date of a custom tag mapping
pub fn get_custom_tag_modified_date(
    conn: &Connection,
    dlsite_tag_name: &str,
) -> Result<Option<String>, HvtError> {
    let date: Option<String> = conn.query_row(
        &format!(
            "SELECT ctm.modified_at
             FROM {DB_CUSTOM_TAG_MAPPINGS_NAME} ctm
             JOIN {DB_DLSITE_TAG_NAME} dt ON ctm.dlsite_tag_id = dt.tag_id
             WHERE dt.tag_name = ?1"
        ),
        params![dlsite_tag_name],
        |row| row.get(0),
    ).ok();

    Ok(date)
}

/// Get all works that use a specific DLSite tag
/// Returns Vec<(rjcode, work_name)>
pub fn get_works_using_tag(
    conn: &Connection,
    dlsite_tag_name: &str,
) -> Result<Vec<(String, String)>, HvtError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT f.rjcode, w.name
             FROM {DB_FOLDERS_NAME} f
             LEFT JOIN {DB_WORKS_NAME} w ON f.fld_id = w.fld_id
             WHERE f.fld_id IN (
                 SELECT fld_id FROM {DB_LKP_WORK_TAG_NAME} WHERE tag_id = (
                     SELECT tag_id FROM {DB_DLSITE_TAG_NAME} WHERE tag_name = ?1
                 )
             )
             ORDER BY f.rjcode"
        )
    )?;

    let works: Vec<(String, String)> = stmt
        .query_map(params![dlsite_tag_name], |row| {
            Ok((
                row.get(0)?,
                row.get::<_, Option<String>>(1)?.unwrap_or_else(|| String::from("Unknown"))
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(works)
}

/// Mark all works using a specific tag for re-tagging
pub fn mark_works_for_retagging(
    conn: &Connection,
    dlsite_tag_name: &str,
) -> Result<usize, HvtError> {
    let rows_affected = conn.execute(
        &format!(
            "UPDATE {DB_FILE_PROCESSING_NAME}
             SET tag_date = NULL, is_tagged = 0
             WHERE fld_id IN (
                 SELECT fld_id FROM {DB_LKP_WORK_TAG_NAME} WHERE tag_id = (
                     SELECT tag_id FROM {DB_DLSITE_TAG_NAME} WHERE tag_name = ?1
                 )
             )"
        ),
        params![dlsite_tag_name],
    )?;

    Ok(rows_affected)
}

/// Check if any tags used by this work have been modified since last tagging
pub fn should_retag_work(conn: &Connection, work: &RJCode) -> Result<bool, HvtError> {
    // Get the last tag date for files in this work
    let file_tag_date: Option<String> = conn.query_row(
        &format!(
            "SELECT MAX(tag_date) FROM {DB_FILE_PROCESSING_NAME}
             WHERE fld_id = (SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1)"
        ),
        params![work.as_str()],
        |row| row.get(0),
    ).ok().flatten();

    // If no tag date, definitely needs tagging
    if file_tag_date.is_none() {
        return Ok(true);
    }

    let file_date = file_tag_date.unwrap();

    // Check if any custom tag mappings for tags used by this work were modified after the file tag date
    let has_newer_mappings: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*)
             FROM {DB_CUSTOM_TAG_MAPPINGS_NAME} ctm
             WHERE ctm.dlsite_tag_id IN (
                 SELECT tag_id FROM {DB_LKP_WORK_TAG_NAME} WHERE fld_id = (
                     SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1
                 )
             )
             AND ctm.modified_at > ?2"
        ),
        params![work.as_str(), file_date],
        |row| row.get(0),
    ).unwrap_or(0);

    Ok(has_newer_mappings > 0)
}

/// List all works with RJCode
pub fn list_all_works(conn: &Connection) -> Result<Vec<(String, String)>, HvtError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT f.rjcode, w.name
             FROM {DB_FOLDERS_NAME} f
             LEFT JOIN {DB_WORKS_NAME} w ON f.fld_id = w.fld_id
             ORDER BY f.rjcode"
        )
    )?;

    let works: Vec<(String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?.unwrap_or_else(|| String::from("Unknown"))
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(works)
}
