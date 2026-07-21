use rusqlite::{params, Connection};

use crate::database::tables::*;
use crate::errors::HvtError;
use crate::folders::types::RJCode;

/// Default sort for `list_all_cvs_with_counts` — alphabetical by JP name.
pub const DEFAULT_CV_SORT: &str = "cv.name_jp COLLATE NOCASE ASC";

/// List all CVs with work counts. `order_by` is a caller-supplied, pre-whitelisted SQL
/// `ORDER BY` fragment (see `web/routes/cvs.rs` for the web UI's column-sort whitelist) — never
/// built from raw user input.
/// Returns Vec<(cv_id, name_jp, name_en, custom_name_if_mapped, work_count)>
pub fn list_all_cvs_with_counts(
    conn: &Connection,
    order_by: &str,
) -> Result<Vec<(i64, String, Option<String>, Option<String>, i64)>, HvtError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT cv.cv_id, cv.name_jp, cv.name_en, ccvm.custom_name, COUNT(lwcv.fld_id) AS work_count
         FROM {DB_CVS_NAME} cv
         LEFT JOIN {DB_CUSTOM_CV_MAPPINGS_NAME} ccvm ON ccvm.cv_id = cv.cv_id
         LEFT JOIN {DB_LKP_WORK_CVS_NAME} lwcv ON lwcv.cv_id = cv.cv_id
         GROUP BY cv.cv_id, cv.name_jp, cv.name_en, ccvm.custom_name
         ORDER BY {order_by}"
    ))?;

    let cvs = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(cvs)
}

/// Add or update a global custom CV mapping (rename). Applies to ALL works featuring this CV.
pub fn add_custom_cv_mapping(conn: &Connection, cv_name_jp: &str, custom_name: &str) -> Result<(), HvtError> {
    let cv_id: i64 = conn.query_row(
        &format!("SELECT cv_id FROM {DB_CVS_NAME} WHERE name_jp = ?1"),
        params![cv_name_jp],
        |row| row.get(0),
    )?;

    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {DB_CUSTOM_CV_MAPPINGS_NAME} (cv_id, custom_name, modified_at)
             VALUES (?1, ?2, datetime('now'))"
        ),
        params![cv_id, custom_name],
    )?;

    Ok(())
}

/// Remove a custom CV mapping (revert to the DLSite name_jp).
pub fn remove_custom_cv_mapping(conn: &Connection, cv_name_jp: &str) -> Result<(), HvtError> {
    let cv_id: i64 = conn.query_row(
        &format!("SELECT cv_id FROM {DB_CVS_NAME} WHERE name_jp = ?1"),
        params![cv_name_jp],
        |row| row.get(0),
    )?;

    conn.execute(
        &format!("DELETE FROM {DB_CUSTOM_CV_MAPPINGS_NAME} WHERE cv_id = ?1"),
        params![cv_id],
    )?;

    Ok(())
}

/// Get merged CVs for a work (DLSite cvs + global custom rename applied), deduped.
/// This is the function the tagger calls instead of reading `cvs.name_jp` raw, so a rename
/// actually reaches the ID3 `artist` tag, not just the web UI display.
pub fn get_merged_cvs_for_work(conn: &Connection, work: &RJCode) -> Result<Vec<String>, HvtError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT COALESCE(ccvm.custom_name, cv.name_jp) AS final_name
         FROM {DB_CVS_NAME} cv
         LEFT JOIN {DB_CUSTOM_CV_MAPPINGS_NAME} ccvm ON ccvm.cv_id = cv.cv_id
         WHERE cv.cv_id IN (
             SELECT cv_id FROM {DB_LKP_WORK_CVS_NAME} WHERE fld_id = (
                 SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1
             )
         )"
    ))?;

    let mut cvs: Vec<String> = stmt
        .query_map(params![work.as_str()], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Dedup in case two merged CVs (e.g. after a manual merge) collapse to the same custom name.
    cvs.sort();
    cvs.dedup();

    Ok(cvs)
}

/// Mark all works featuring a specific CV for re-tagging.
pub fn mark_works_for_retagging(conn: &Connection, cv_name_jp: &str) -> Result<usize, HvtError> {
    let rows_affected = conn.execute(
        &format!(
            "UPDATE {DB_FILE_PROCESSING_NAME}
             SET tag_date = NULL, is_tagged = 0
             WHERE fld_id IN (
                 SELECT fld_id FROM {DB_LKP_WORK_CVS_NAME} WHERE cv_id = (
                     SELECT cv_id FROM {DB_CVS_NAME} WHERE name_jp = ?1
                 )
             )"
        ),
        params![cv_name_jp],
    )?;

    Ok(rows_affected)
}

/// Check if a work needs re-tagging because a CV rename affecting it happened after last tagging.
pub fn should_retag_work_for_cv(conn: &Connection, work: &RJCode) -> Result<bool, HvtError> {
    let file_tag_date: Option<String> = conn
        .query_row(
            &format!(
                "SELECT MAX(tag_date) FROM {DB_FILE_PROCESSING_NAME}
                 WHERE fld_id = (SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1)"
            ),
            params![work.as_str()],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    let Some(file_date) = file_tag_date else {
        return Ok(true);
    };

    let has_newer_mapping: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*)
                 FROM {DB_CUSTOM_CV_MAPPINGS_NAME} ccvm
                 WHERE ccvm.cv_id IN (
                     SELECT cv_id FROM {DB_LKP_WORK_CVS_NAME} WHERE fld_id = (
                         SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1
                     )
                 )
                 AND ccvm.modified_at > ?2"
            ),
            params![work.as_str(), file_date],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(has_newer_mapping > 0)
}
