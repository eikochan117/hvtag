use rusqlite::{Connection, params};
use crate::folders::types::{ManagedFolder, RGCode, RJCode};
use crate::database::tables::*;
use crate::errors::HvtError;
use crate::tagger::track_parser::TrackParsingPreference;

/// Insert a managed folder into the database
pub fn insert_managed_folder(
    conn: &Connection,
    mf: &ManagedFolder,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
           "WITH mx AS (SELECT COALESCE(MAX(fld_id), 0) AS m FROM {DB_FOLDERS_NAME}) 
            INSERT OR IGNORE INTO {DB_FOLDERS_NAME} (fld_id, rjcode, path, last_scan, active)
            SELECT mx.m + 1, ?1, ?2, datetime(), ?3
            FROM mx"),
        params![&mf.rjcode, &mf.path, true],
    )?;
    Ok(rows)
}

/// Insert an error for a work
pub fn insert_error(
    conn: &Connection,
    work: &RJCode,
    error: &str,
    error_category: Option<&str>,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "INSERT INTO {DB_DLSITE_ERRORS_NAME} (fld_id, error_type, error_category, error_timestamp)
             SELECT fld_id, ?1, ?2, CURRENT_TIMESTAMP
             FROM {DB_FOLDERS_NAME}
             WHERE rjcode = ?3"
        ),
        params![error, error_category, work],
    )?;
    Ok(rows)
}

/// Insert a tag
pub fn insert_tag(
    conn: &Connection,
    tag: &str,
    tag_id: usize,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!("INSERT OR IGNORE INTO {DB_DLSITE_TAG_NAME} (tag_id, tag_name) VALUES (?1, ?2)"),
        params![tag_id, tag],
    )?;
    Ok(rows)
}

/// Check if a circle already exists in the database
pub fn circle_exists(
    conn: &Connection,
    circle: &RGCode,
) -> Result<bool, HvtError> {
    let count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {DB_CIRCLE_NAME} WHERE rgcode = ?1"),
        params![circle],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Insert a circle
pub fn insert_circle(
    conn: &Connection,
    circle: &RGCode,
    en_name: &str,
    jp_name: &str,
    cir_id: usize,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!("INSERT OR REPLACE INTO {DB_CIRCLE_NAME} (cir_id, rgcode, name_en, name_jp) VALUES (?1, ?2, ?3, ?4)"),
        params![cir_id, circle, en_name, jp_name],
    )?;
    Ok(rows)
}

/// Insert a CV (voice actor), looked up by its natural key (`name_jp`) FIRST so a
/// re-encountered actor reuses their existing cv_id instead of minting a new one and
/// triggering `INSERT OR REPLACE`'s delete-then-insert conflict path (which cascades and
/// deletes every other work's lkp_work_cvs row for that actor). Returns the cv_id: the
/// existing row's id if `name_jp` already exists, otherwise the id assigned by SQLite's
/// native `INTEGER PRIMARY KEY` autoincrement.
pub fn insert_cv(
    conn: &Connection,
    jp_name: &str,
    en_name: &str,
) -> Result<i64, HvtError> {
    let existing: Option<i64> = conn
        .query_row(
            &format!("SELECT cv_id FROM {DB_CVS_NAME} WHERE name_jp = ?1"),
            params![jp_name],
            |row| row.get(0),
        )
        .ok();

    if let Some(cv_id) = existing {
        return Ok(cv_id);
    }

    conn.execute(
        &format!("INSERT INTO {DB_CVS_NAME} (name_jp, name_en) VALUES (?1, ?2)"),
        params![jp_name, en_name],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Narrow, unambiguous CV-name normalization applied before any DB lookup/insert: only
/// collapses full-width parentheses （）(U+FF08/U+FF09) to their half-width ASCII equivalents
/// () and trims whitespace. Deliberately does NOT strip parenthetical content (e.g. a
/// "(real name)" suffix) and does NOT fold kana spelling variants — both are ambiguous
/// judgment calls left entirely to the manual custom_cv_mappings merge UI.
pub fn normalize_cv_name(name: &str) -> String {
    name.replace('（', "(").replace('）', ")").trim().to_string()
}

/// Remove previous data of a work from a table
pub fn remove_previous_data_of_work(
    conn: &Connection,
    table: &str,
    work: &RJCode,
) -> Result<usize, HvtError> {
    let sql = format!(
        "DELETE FROM {table}
         WHERE fld_id IN (
             SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1
         )"
    );
    let rows = conn.execute(&sql, params![work])?;
    Ok(rows)
}

/// Assign release date to a work
pub fn assign_release_date_to_work(
    conn: &Connection,
    work: &RJCode,
    date: &str,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "INSERT INTO {DB_RELEASE_DATE_NAME} (fld_id, release_date)
             SELECT fld_id, ?1 
             FROM {DB_FOLDERS_NAME}
             WHERE rjcode = ?2"
        ),
        params![date, work],
    )?;
    Ok(rows)
}

/// Assign circle to a work
pub fn assign_circle_to_work(
    conn: &Connection,
    work: &RJCode,
    circle: &RGCode,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "INSERT INTO {DB_LKP_WORK_CIRCLE_NAME} (fld_id, cir_id)
             SELECT t1.fld_id, t2.cir_id
             FROM {DB_FOLDERS_NAME} t1, {DB_CIRCLE_NAME} t2
             WHERE t1.rjcode = ?1 AND t2.rgcode = ?2"
        ),
        params![work, circle],
    )?;
    Ok(rows)
}

/// Assign tags to a work
pub fn assign_tags_to_work(
    conn: &Connection,
    work: &RJCode,
    tags: &[String],
) -> Result<usize, HvtError> {
    if tags.is_empty() {
        return Ok(0);
    }

    // Build placeholders for IN clause
    let placeholders: Vec<String> = (0..tags.len()).map(|i| format!("?{}", i + 2)).collect();
    let placeholders_str = placeholders.join(", ");

    let sql = format!(
        "INSERT INTO {DB_LKP_WORK_TAG_NAME} (fld_id, tag_id)
         SELECT t1.fld_id, t2.tag_id
         FROM {DB_FOLDERS_NAME} t1, {DB_DLSITE_TAG_NAME} t2
         WHERE t1.rjcode = ?1 AND t2.tag_name IN ({placeholders_str})"
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![work];
    for tag in tags {
        params_vec.push(tag);
    }
    let rows = stmt.execute(params_vec.as_slice())?;
    Ok(rows)
}

/// Assign rating to a work
pub fn assign_rating_to_work(
    conn: &Connection,
    work: &RJCode,
    rating: &str,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "INSERT INTO {DB_RATING_NAME} (fld_id, rating)
             SELECT fld_id, ?1
             FROM {DB_FOLDERS_NAME}
             WHERE rjcode = ?2"
        ),
        params![rating, work],
    )?;
    Ok(rows)
}

/// Assign stars rating to a work
pub fn assign_stars_to_work(
    conn: &Connection,
    work: &RJCode,
    stars: f32,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "INSERT INTO {DB_STARS_NAME} (fld_id, stars)
             SELECT fld_id, ?1
             FROM {DB_FOLDERS_NAME}
             WHERE rjcode = ?2"
        ),
        params![stars, work],
    )?;
    Ok(rows)
}

/// Assign cover link to a work
pub fn assign_cover_link_to_work(
    conn: &Connection,
    work: &RJCode,
    link: &str,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "INSERT INTO {DB_DLSITE_COVERS_LINK_NAME} (fld_id, link)
             SELECT fld_id, ?1
             FROM {DB_FOLDERS_NAME}
             WHERE rjcode = ?2"
        ),
        params![link, work],
    )?;
    Ok(rows)
}

/// Assign CVs to a work
pub fn assign_cvs_to_work(
    conn: &Connection,
    work: &RJCode,
    cvs: &[String],
) -> Result<usize, HvtError> {
    if cvs.is_empty() {
        return Ok(0);
    }

    // Build placeholders for IN clause
    let placeholders: Vec<String> = (0..cvs.len()).map(|i| format!("?{}", i + 2)).collect();
    let placeholders_str = placeholders.join(", ");

    let sql = format!(
        "INSERT INTO {DB_LKP_WORK_CVS_NAME} (fld_id, cv_id)
         SELECT t1.fld_id, t2.cv_id
         FROM {DB_FOLDERS_NAME} t1, {DB_CVS_NAME} t2
         WHERE t1.rjcode = ?1 AND t2.name_jp IN ({placeholders_str})"
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![work];
    for cv in cvs {
        params_vec.push(cv);
    }
    let rows = stmt.execute(params_vec.as_slice())?;
    Ok(rows)
}

/// Insert or update work name in the works table
pub fn insert_work_name(
    conn: &Connection,
    work: &RJCode,
    work_name: &str,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {DB_WORKS_NAME} (fld_id, name)
             SELECT fld_id, ?2
             FROM {DB_FOLDERS_NAME}
             WHERE rjcode = ?1"
        ),
        params![work, work_name],
    )?;
    Ok(rows)
}

/// Set work scan date
pub fn set_work_scan_date(
    conn: &Connection,
    work: &RJCode,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {DB_DLSITE_SCAN_NAME} (fld_id, last_scan)
             SELECT fld_id, datetime()
             FROM {DB_FOLDERS_NAME}
             WHERE rjcode = ?1"
        ),
        params![work],
    )?;
    Ok(rows)
}

/// Get maximum ID from a table
pub fn get_max_id(
    conn: &Connection,
    id_fld: &str,
    table: &str,
) -> Result<usize, HvtError> {
    let sql = format!("SELECT COALESCE(MAX({id_fld}), 0) FROM {table}");
    let max_id: usize = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(max_id)
}

/// Get all active works with their registered paths — used by `--full-retag` to enumerate
/// every work in the library.
pub fn get_all_works_with_paths(conn: &Connection) -> Result<Vec<(RJCode, String)>, HvtError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT rjcode, path FROM {DB_FOLDERS_NAME} WHERE active = 1"
    ))?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let works: Vec<(RJCode, String)> = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(works)
}

/// Get the registered folder path for a specific work, if it exists in the database.
/// Used by `--retag <rjcode>` to resolve the real library path rather than assuming cwd.
pub fn get_work_path(conn: &Connection, rjcode: &RJCode) -> Result<Option<String>, HvtError> {
    let path: Option<String> = conn
        .query_row(
            &format!("SELECT path FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1"),
            params![rjcode],
            |row| row.get(0),
        )
        .ok();
    Ok(path)
}

/// Check if a work is already registered in the database — used by `--tag <folder>` to refuse
/// running its one-shot test mode against an already-imported work (see `rjcode_exists`'s
/// counterpart usage: that path temporarily inserts then deletes a folder row, which would be
/// unsafe to run against a real, pre-existing work).
pub fn rjcode_exists(conn: &Connection, rjcode: &RJCode) -> Result<bool, HvtError> {
    let count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1"),
        params![rjcode],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Permanently removes a work from the database (no filesystem changes) — for works whose folder
/// is already gone from disk, where the trash feature's file-move step doesn't apply. Unlike
/// `deactivate_and_relocate_work` (the reversible trash path), this is NOT reversible: every
/// child row is gone for good. `file_processing` has no `ON DELETE CASCADE` on `fld_id` (see
/// `tables.rs`), so it must be deleted explicitly first; everything else under `folders.fld_id`
/// (works, lkp_work_tag/circle/cvs, rating, stars, release_date, dlsite_covers, dlsite_scan,
/// track_parsing_prefs) cascades from the final `folders` delete.
pub fn delete_work_permanently(conn: &Connection, rjcode: &RJCode) -> Result<(), HvtError> {
    conn.execute(
        &format!(
            "DELETE FROM {DB_FILE_PROCESSING_NAME} WHERE fld_id = (SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1)"
        ),
        params![rjcode],
    )?;
    conn.execute(
        &format!("DELETE FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1"),
        params![rjcode],
    )?;
    Ok(())
}

/// Get all unscanned works with their paths from the database
pub fn get_unscanned_works_with_paths(conn: &Connection) -> Result<Vec<(RJCode, String)>, HvtError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT rjcode, path FROM {DB_FOLDERS_NAME}
         WHERE fld_id NOT IN (SELECT fld_id FROM {DB_WORKS_NAME})"
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    let works: Vec<(RJCode, String)> = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(works)
}

/// Get cover link for a specific work
pub fn get_cover_link(conn: &Connection, rjcode: &RJCode) -> Result<Option<String>, HvtError> {
    let result = conn.query_row(
        &format!(
            "SELECT dc.link
             FROM {DB_FOLDERS_NAME} f
             INNER JOIN {DB_DLSITE_COVERS_LINK_NAME} dc ON f.fld_id = dc.fld_id
             WHERE f.rjcode = ?1 AND dc.link IS NOT NULL"
        ),
        params![rjcode],
        |row| row.get(0),
    );

    match result {
        Ok(link) => Ok(Some(link)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Get track parsing preference for a work
pub fn get_track_parsing_preference(
    conn: &Connection,
    rjcode: &RJCode,
) -> Result<Option<TrackParsingPreference>, HvtError> {
    let result = conn.query_row(
        &format!(
            "SELECT strategy_name, custom_delimiter, use_asian_conversion, asian_format_type,
                    strip_prefix_pattern
             FROM {DB_TRACK_PARSING_PREFS_NAME}
             WHERE fld_id = (SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1)"
        ),
        params![rjcode],
        |row| {
            Ok(TrackParsingPreference {
                strategy_name: row.get(0)?,
                custom_delimiter: row.get(1)?,
                use_asian_conversion: row.get::<_, i64>(2)? != 0,
                asian_format_type: row.get(3)?,
                strip_prefix_pattern: row.get(4)?,
            })
        },
    );

    match result {
        Ok(pref) => Ok(Some(pref)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Save track parsing preference for a work
pub fn save_track_parsing_preference(
    conn: &Connection,
    rjcode: &RJCode,
    preference: &TrackParsingPreference,
) -> Result<(), HvtError> {
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {DB_TRACK_PARSING_PREFS_NAME}
             (fld_id, strategy_name, custom_delimiter, use_asian_conversion, asian_format_type,
              strip_prefix_pattern, last_used)
             VALUES (
                 (SELECT fld_id FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1),
                 ?2, ?3, ?4, ?5, ?6, datetime('now')
             )"
        ),
        params![
            rjcode,
            &preference.strategy_name,
            &preference.custom_delimiter,
            preference.use_asian_conversion,
            &preference.asian_format_type,
            &preference.strip_prefix_pattern,
        ],
    )?;

    Ok(())
}

/// Update folder path for a work in database
pub fn update_folder_path(
    conn: &Connection,
    rjcode: &RJCode,
    new_path: &str,
) -> Result<usize, HvtError> {
    let rows = conn.execute(
        &format!(
            "UPDATE {DB_FOLDERS_NAME}
             SET path = ?1
             WHERE rjcode = ?2"
        ),
        params![new_path, rjcode],
    )?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_cv_name_collapses_paren_width() {
        // The two real near-duplicate DB rows differing only by paren width must normalize
        // to the exact same string.
        assert_eq!(
            normalize_cv_name("乙倉ゅい(乙倉由依)"),
            normalize_cv_name("乙倉ゅい（乙倉由依）"),
        );
        assert_eq!(normalize_cv_name("乙倉ゅい（乙倉由依）"), "乙倉ゅい(乙倉由依)");
    }

    #[test]
    fn test_normalize_cv_name_leaves_ambiguous_variants_distinct() {
        // Kana spelling variant (ゅ vs ゆ) is intentionally NOT folded - ambiguous, left to
        // the manual custom_cv_mappings merge UI.
        assert_ne!(normalize_cv_name("乙倉ゅい"), normalize_cv_name("乙倉ゆい"));
        // Name-presence variant (with vs without a real-name gloss) is intentionally NOT
        // stripped either.
        assert_ne!(normalize_cv_name("MOMOKA。"), normalize_cv_name("MOMOKA。（柚木桃香）"));
    }

    #[test]
    fn test_normalize_cv_name_trims_whitespace() {
        assert_eq!(normalize_cv_name("  Nodoka Nishiura  "), "Nodoka Nishiura");
    }
}
