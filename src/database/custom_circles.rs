use rusqlite::{Connection, params};
use crate::errors::HvtError;
use crate::folders::types::RJCode;
use crate::database::tables::*;

/// Circle preference type - how to display circle name in audio tags
#[derive(Debug, Clone, PartialEq)]
pub enum CirclePreferenceType {
    ForceEn,   // Always use name_en
    ForceJp,   // Always use name_jp
    Custom,    // Use custom_name
    UseCode,   // Use rgcode (RG12345)
}

impl CirclePreferenceType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "force_en" => Some(CirclePreferenceType::ForceEn),
            "force_jp" => Some(CirclePreferenceType::ForceJp),
            "custom" => Some(CirclePreferenceType::Custom),
            "use_code" => Some(CirclePreferenceType::UseCode),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CirclePreferenceType::ForceEn => "force_en",
            CirclePreferenceType::ForceJp => "force_jp",
            CirclePreferenceType::Custom => "custom",
            CirclePreferenceType::UseCode => "use_code",
        }
    }
}

/// List all circles in the database (alphabetically sorted)
/// Returns Vec<(cir_id, rgcode, name_en, name_jp, pref_type?, custom_name?)>
pub fn list_all_circles(conn: &Connection) -> Result<Vec<(i64, String, String, String, Option<String>, Option<String>)>, HvtError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT c.cir_id, c.rgcode, c.name_en, c.name_jp, ccm.preference_type, ccm.custom_name
             FROM {DB_CIRCLE_NAME} c
             LEFT JOIN {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} ccm ON c.cir_id = ccm.cir_id
             ORDER BY
                 CASE
                     WHEN c.name_jp IS NOT NULL AND c.name_jp != '' THEN c.name_jp
                     WHEN c.name_en IS NOT NULL AND c.name_en != '' THEN c.name_en
                     ELSE c.rgcode
                 END ASC"
        )
    )?;

    let circles: Vec<(i64, String, String, String, Option<String>, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(circles)
}

/// Set circle preference (global mapping)
/// This applies to ALL works by this circle
pub fn set_circle_preference(
    conn: &Connection,
    rgcode: &str,
    preference: CirclePreferenceType,
    custom_name: Option<&str>,
) -> Result<(), HvtError> {
    // Get the cir_id for this circle
    let cir_id: i64 = conn.query_row(
        &format!("SELECT cir_id FROM {DB_CIRCLE_NAME} WHERE rgcode = ?1"),
        params![rgcode],
        |row| row.get(0),
    )?;

    // Validate: custom_name must be provided if preference is Custom
    if preference == CirclePreferenceType::Custom && custom_name.is_none() {
        return Err(HvtError::Database(rusqlite::Error::InvalidParameterName(
            "custom_name required for Custom preference type".to_string()
        )));
    }

    // Insert or replace the preference
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {DB_CUSTOM_CIRCLE_MAPPINGS_NAME}
             (cir_id, preference_type, custom_name, modified_at)
             VALUES (?1, ?2, ?3, datetime('now'))"
        ),
        params![cir_id, preference.as_str(), custom_name],
    )?;

    Ok(())
}

/// Remove circle preference (revert to default JP → EN → Unknown)
pub fn remove_circle_preference(
    conn: &Connection,
    rgcode: &str,
) -> Result<(), HvtError> {
    // Get the cir_id for this circle
    let cir_id: i64 = conn.query_row(
        &format!("SELECT cir_id FROM {DB_CIRCLE_NAME} WHERE rgcode = ?1"),
        params![rgcode],
        |row| row.get(0),
    )?;

    conn.execute(
        &format!("DELETE FROM {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} WHERE cir_id = ?1"),
        params![cir_id],
    )?;

    Ok(())
}

/// Get merged circle name for a work (with custom preference applied)
/// This is the CORE function used by the tagger
pub fn get_merged_circle_name_for_work(
    conn: &Connection,
    work: &RJCode,
) -> Result<String, HvtError> {
    let circle_name: String = conn.query_row(
        &format!(
            "SELECT
                CASE
                    WHEN ccm.preference_type = 'force_en' THEN c.name_en
                    WHEN ccm.preference_type = 'force_jp' THEN c.name_jp
                    WHEN ccm.preference_type = 'custom' THEN ccm.custom_name
                    WHEN ccm.preference_type = 'use_code' THEN c.rgcode
                    ELSE COALESCE(NULLIF(c.name_jp, ''), c.name_en, 'Unknown Circle')
                END as final_name
             FROM {DB_CIRCLE_NAME} c
             LEFT JOIN {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} ccm ON c.cir_id = ccm.cir_id
             WHERE c.cir_id IN (
                 SELECT cir_id FROM {DB_LKP_WORK_CIRCLE_NAME} WHERE fld_id = (
                     SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1
                 )
             )
             LIMIT 1"
        ),
        params![work.as_str()],
        |row| row.get(0),
    ).unwrap_or_else(|_| String::from("Unknown Circle"));

    Ok(circle_name)
}

/// Get all works by a specific circle
/// Returns Vec<(rjcode, work_name)>
pub fn get_works_using_circle(
    conn: &Connection,
    rgcode: &str,
) -> Result<Vec<(String, String)>, HvtError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT f.rjcode, w.name
             FROM {DB_FOLDERS_NAME} f
             LEFT JOIN {DB_WORKS_NAME} w ON f.fld_id = w.fld_id
             WHERE f.fld_id IN (
                 SELECT fld_id FROM {DB_LKP_WORK_CIRCLE_NAME} WHERE cir_id = (
                     SELECT cir_id FROM {DB_CIRCLE_NAME} WHERE rgcode = ?1
                 )
             )
             ORDER BY f.rjcode"
        )
    )?;

    let works: Vec<(String, String)> = stmt
        .query_map(params![rgcode], |row| {
            Ok((
                row.get(0)?,
                row.get::<_, Option<String>>(1)?.unwrap_or_else(|| String::from("Unknown"))
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(works)
}

/// Mark all works by a specific circle for re-tagging
pub fn mark_circle_works_for_retagging(
    conn: &Connection,
    rgcode: &str,
) -> Result<usize, HvtError> {
    let rows_affected = conn.execute(
        &format!(
            "UPDATE {DB_FILE_PROCESSING_NAME}
             SET tag_date = NULL, is_tagged = 0
             WHERE fld_id IN (
                 SELECT fld_id FROM {DB_LKP_WORK_CIRCLE_NAME} WHERE cir_id = (
                     SELECT cir_id FROM {DB_CIRCLE_NAME} WHERE rgcode = ?1
                 )
             )"
        ),
        params![rgcode],
    )?;

    Ok(rows_affected)
}

/// Check if a work needs re-tagging due to circle preference changes
pub fn should_retag_work_for_circle(conn: &Connection, work: &RJCode) -> Result<bool, HvtError> {
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

    // Check if circle preference for this work was modified after the file tag date
    let has_newer_mapping: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*)
             FROM {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} ccm
             WHERE ccm.cir_id IN (
                 SELECT cir_id FROM {DB_LKP_WORK_CIRCLE_NAME} WHERE fld_id = (
                     SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1
                 )
             )
             AND ccm.modified_at > ?2"
        ),
        params![work.as_str(), file_date],
        |row| row.get(0),
    ).unwrap_or(0);

    Ok(has_newer_mapping > 0)
}

/// Get all custom circle preferences
/// Returns Vec<(rgcode, name_en, name_jp, preference_type, custom_name)>
pub fn get_all_custom_circle_preferences(conn: &Connection) -> Result<Vec<(String, String, String, String, Option<String>)>, HvtError> {
    let mut stmt = conn.prepare(
        &format!(
            "SELECT c.rgcode, c.name_en, c.name_jp, ccm.preference_type, ccm.custom_name
             FROM {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} ccm
             JOIN {DB_CIRCLE_NAME} c ON ccm.cir_id = c.cir_id
             ORDER BY
                 CASE
                     WHEN c.name_jp IS NOT NULL AND c.name_jp != '' THEN c.name_jp
                     WHEN c.name_en IS NOT NULL AND c.name_en != '' THEN c.name_en
                     ELSE c.rgcode
                 END ASC"
        )
    )?;

    let prefs: Vec<(String, String, String, String, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(prefs)
}

/// Get circle information by RG code
/// Returns (cir_id, rgcode, name_en, name_jp)
pub fn get_circle_info(conn: &Connection, rgcode: &str) -> Result<(i64, String, String, String), HvtError> {
    let info: (i64, String, String, String) = conn.query_row(
        &format!(
            "SELECT cir_id, rgcode, name_en, name_jp
             FROM {DB_CIRCLE_NAME}
             WHERE rgcode = ?1"
        ),
        params![rgcode],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        },
    )?;

    Ok(info)
}

/// Get current preference for a circle (if any)
/// Returns (preference_type, custom_name)
pub fn get_circle_preference(conn: &Connection, rgcode: &str) -> Result<Option<(String, Option<String>)>, HvtError> {
    let pref: Option<(String, Option<String>)> = conn.query_row(
        &format!(
            "SELECT ccm.preference_type, ccm.custom_name
             FROM {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} ccm
             WHERE ccm.cir_id = (
                 SELECT cir_id FROM {DB_CIRCLE_NAME} WHERE rgcode = ?1
             )"
        ),
        params![rgcode],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
            ))
        },
    ).ok();

    Ok(pref)
}
