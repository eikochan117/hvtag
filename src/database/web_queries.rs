use rusqlite::{params, Connection};

use crate::database::custom_circles;
use crate::database::custom_cvs;
use crate::database::custom_tags;
use crate::database::tables::*;
use crate::errors::HvtError;
use crate::folders::types::RJCode;

/// One row in the works list (used by both the full-page load and the htmx search partial).
#[derive(Debug, Clone)]
pub struct WorkSummary {
    pub rjcode: String,
    pub name: String,
    pub circle_name: String,
    pub stars: Option<f32>,
}

/// Full metadata for the work detail page.
#[derive(Debug, Clone)]
pub struct WorkDetail {
    pub rjcode: String,
    pub name: String,
    pub circle_name: String,
    pub circle_rgcode: Option<String>,
    pub folder_path: String,
    pub tags: Vec<String>,
    pub cvs: Vec<String>,
    pub rating: Option<String>,
    pub stars: Option<f32>,
    pub release_date: Option<String>,
}

/// Filters for the works list: `q` is a free-text substring match (existing behavior); `tag`/
/// `circle`/`cv` are optional *exact* matches, composable with `q` and with each other, used for
/// click-through navigation (e.g. clicking a tag chip filters to exactly the works that have it).
/// - `tag`: exact merged/display tag name — same semantics as `custom_tags::get_merged_tags_for_work`
///   (custom rename applied, ignored tags excluded).
/// - `circle`: exact `circles.rgcode` — the stable key (display names can collide under custom prefs).
/// - `cv`: exact merged/display CV name — same semantics as `custom_cvs::get_merged_cvs_for_work`.
pub struct WorkFilter<'a> {
    pub q: &'a str,
    pub tag: Option<&'a str>,
    pub circle: Option<&'a str>,
    pub cv: Option<&'a str>,
}

/// Sort order for the works list dropdown/column headers. `Rating` sorts by the DLSite star
/// score (highest first, works with no score last) — the number actually shown on every work
/// card — not the separate age-category `rating` DB column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkSort {
    Title,
    Circle,
    Rjcode,
    Rating,
}

impl WorkSort {
    pub fn from_param(s: Option<&str>) -> Self {
        match s {
            Some("circle") => WorkSort::Circle,
            Some("rjcode") => WorkSort::Rjcode,
            Some("rating") => WorkSort::Rating,
            _ => WorkSort::Title,
        }
    }

    /// The value used in `?sort=` query strings and matched by `from_param` above.
    pub fn as_param(&self) -> &'static str {
        match self {
            WorkSort::Title => "title",
            WorkSort::Circle => "circle",
            WorkSort::Rjcode => "rjcode",
            WorkSort::Rating => "rating",
        }
    }

    fn order_by_sql(&self) -> &'static str {
        match self {
            WorkSort::Title => "name COLLATE NOCASE ASC",
            WorkSort::Circle => "circle_name COLLATE NOCASE ASC, name COLLATE NOCASE ASC",
            WorkSort::Rjcode => "f.rjcode ASC",
            WorkSort::Rating => "s.stars IS NULL ASC, s.stars DESC, name COLLATE NOCASE ASC",
        }
    }
}

/// The shared filter WHERE clause: free-text `q` match (RJcode, title, circle name, tag name) AND
/// the optional exact tag/circle/cv filters. `(?N IS NULL OR ...)` lets `Option<&str>` bind
/// straight to SQL NULL via rusqlite's params! macro when a filter isn't active — no dynamic SQL
/// string building needed.
const FILTER_WHERE: &str = "
    f.active = 1
    AND (
        ?1 = ''
        OR f.rjcode LIKE '%' || ?1 || '%'
        OR w.name LIKE '%' || ?1 || '%'
        OR c.name_en LIKE '%' || ?1 || '%'
        OR c.name_jp LIKE '%' || ?1 || '%'
        OR f.fld_id IN (
            SELECT lwt.fld_id FROM lkp_work_tag lwt
            JOIN dlsite_tag dt ON dt.tag_id = lwt.tag_id
            LEFT JOIN custom_tag_mappings ctm ON ctm.dlsite_tag_id = dt.tag_id
            WHERE dt.tag_name LIKE '%' || ?1 || '%' OR ctm.custom_tag_name LIKE '%' || ?1 || '%'
        )
    )
    AND (?2 IS NULL OR c.rgcode = ?2)
    AND (?3 IS NULL OR EXISTS (
        SELECT 1 FROM lkp_work_tag lwt3
        JOIN dlsite_tag dt3 ON dt3.tag_id = lwt3.tag_id
        LEFT JOIN custom_tag_mappings ctm3 ON ctm3.dlsite_tag_id = dt3.tag_id
        WHERE lwt3.fld_id = f.fld_id
          AND COALESCE(ctm3.is_ignored, 0) = 0
          AND COALESCE(ctm3.custom_tag_name, dt3.tag_name) = ?3
    ))
    AND (?4 IS NULL OR EXISTS (
        SELECT 1 FROM lkp_work_cvs lwcv4
        JOIN cvs cv4 ON cv4.cv_id = lwcv4.cv_id
        LEFT JOIN custom_cv_mappings ccvm4 ON ccvm4.cv_id = cv4.cv_id
        WHERE lwcv4.fld_id = f.fld_id AND COALESCE(ccvm4.custom_name, cv4.name_jp) = ?4
    ))
";

fn merged_circle_name_expr() -> &'static str {
    "COALESCE(
        CASE ccm.preference_type
            WHEN 'force_en' THEN c.name_en
            WHEN 'force_jp' THEN c.name_jp
            WHEN 'custom' THEN ccm.custom_name
            WHEN 'use_code' THEN c.rgcode
            ELSE NULL
        END,
        NULLIF(c.name_jp, ''), c.name_en, 'Unknown Circle'
    )"
}

/// Search + paginate the works list. `filter.q` is a plain substring (no wildcard escaping in
/// v1 — personal-library scale, and `%`/`_` in a search term is a rare, harmless quirk here
/// since queries are parameterized, not a SQL injection surface).
pub fn list_work_summaries(
    conn: &Connection,
    filter: &WorkFilter,
    sort: WorkSort,
    limit: i64,
    offset: i64,
) -> Result<Vec<WorkSummary>, HvtError> {
    let sql = format!(
        "SELECT f.rjcode, COALESCE(w.name, f.rjcode) AS name, {circle_expr} AS circle_name, s.stars
         FROM {DB_FOLDERS_NAME} f
         LEFT JOIN {DB_WORKS_NAME} w ON w.fld_id = f.fld_id
         LEFT JOIN {DB_LKP_WORK_CIRCLE_NAME} lwc ON lwc.fld_id = f.fld_id
         LEFT JOIN {DB_CIRCLE_NAME} c ON c.cir_id = lwc.cir_id
         LEFT JOIN {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} ccm ON ccm.cir_id = c.cir_id
         LEFT JOIN {DB_STARS_NAME} s ON s.fld_id = f.fld_id
         WHERE {FILTER_WHERE}
         GROUP BY f.fld_id
         ORDER BY {order_by}
         LIMIT ?5 OFFSET ?6",
        circle_expr = merged_circle_name_expr(),
        order_by = sort.order_by_sql(),
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![filter.q, filter.circle, filter.tag, filter.cv, limit, offset],
        |row| {
            Ok(WorkSummary {
                rjcode: row.get(0)?,
                name: row.get(1)?,
                circle_name: row.get(2)?,
                stars: row.get(3)?,
            })
        },
    )?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Total number of works matching `filter` (for pagination controls).
pub fn count_work_summaries(conn: &Connection, filter: &WorkFilter) -> Result<i64, HvtError> {
    let sql = format!(
        "SELECT COUNT(DISTINCT f.fld_id)
         FROM {DB_FOLDERS_NAME} f
         LEFT JOIN {DB_WORKS_NAME} w ON w.fld_id = f.fld_id
         LEFT JOIN {DB_LKP_WORK_CIRCLE_NAME} lwc ON lwc.fld_id = f.fld_id
         LEFT JOIN {DB_CIRCLE_NAME} c ON c.cir_id = lwc.cir_id
         WHERE {FILTER_WHERE}"
    );

    let count: i64 = conn.query_row(
        &sql,
        params![filter.q, filter.circle, filter.tag, filter.cv],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Full detail for a single work, or `None` if the RJcode isn't in the database.
/// Reuses the existing merge helpers (`get_merged_tags_for_work`,
/// `get_merged_circle_name_for_work`) rather than re-deriving that logic here.
pub fn get_work_detail(conn: &Connection, rjcode: &RJCode) -> Result<Option<WorkDetail>, HvtError> {
    let base: Option<(i64, String, String)> = conn
        .query_row(
            &format!(
                "SELECT f.fld_id, COALESCE(w.name, f.rjcode), f.path
                 FROM {DB_FOLDERS_NAME} f
                 LEFT JOIN {DB_WORKS_NAME} w ON w.fld_id = f.fld_id
                 WHERE f.rjcode = ?1"
            ),
            params![rjcode],
            |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, Option<String>>(2)?.unwrap_or_default())),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e),
        })?;

    let Some((fld_id, name, folder_path)) = base else {
        return Ok(None);
    };

    let circle_rgcode: Option<String> = conn
        .query_row(
            &format!(
                "SELECT c.rgcode FROM {DB_CIRCLE_NAME} c
                 JOIN {DB_LKP_WORK_CIRCLE_NAME} lwc ON lwc.cir_id = c.cir_id
                 WHERE lwc.fld_id = ?1
                 LIMIT 1"
            ),
            params![fld_id],
            |row| row.get(0),
        )
        .ok();

    let rating: Option<String> = conn
        .query_row(
            &format!("SELECT rating FROM {DB_RATING_NAME} WHERE fld_id = ?1"),
            params![fld_id],
            |row| row.get(0),
        )
        .ok();

    let stars: Option<f32> = conn
        .query_row(
            &format!("SELECT stars FROM {DB_STARS_NAME} WHERE fld_id = ?1"),
            params![fld_id],
            |row| row.get(0),
        )
        .ok();

    let release_date: Option<String> = conn
        .query_row(
            &format!("SELECT release_date FROM {DB_RELEASE_DATE_NAME} WHERE fld_id = ?1"),
            params![fld_id],
            |row| row.get(0),
        )
        .ok();

    let tags = custom_tags::get_merged_tags_for_work(conn, rjcode)?;
    let circle_name = custom_circles::get_merged_circle_name_for_work(conn, rjcode)?;
    let cvs = custom_cvs::get_merged_cvs_for_work(conn, rjcode)?;

    Ok(Some(WorkDetail {
        rjcode: rjcode.as_str().to_string(),
        name,
        circle_name,
        circle_rgcode,
        folder_path,
        tags,
        cvs,
        rating,
        stars,
        release_date,
    }))
}

/// The work's folder path, used to locate `folder.jpeg` for cover serving.
pub fn get_folder_path(conn: &Connection, rjcode: &str) -> Result<Option<String>, HvtError> {
    let path: Option<String> = conn
        .query_row(
            &format!("SELECT path FROM {DB_FOLDERS_NAME} WHERE rjcode = ?1"),
            params![rjcode],
            |row| row.get(0),
        )
        .ok();
    Ok(path)
}

/// Resolves a DLSite tag's numeric id to its name — mutation routes take an id (not a name)
/// to avoid URL-encoding arbitrary Japanese text, then bridge back to the `&str`-based
/// `custom_tags` functions via this lookup.
pub fn get_tag_name_by_id(conn: &Connection, tag_id: i64) -> Result<Option<String>, HvtError> {
    let name: Option<String> = conn
        .query_row(
            &format!("SELECT tag_name FROM {DB_DLSITE_TAG_NAME} WHERE tag_id = ?1"),
            params![tag_id],
            |row| row.get(0),
        )
        .ok();
    Ok(name)
}

/// Resolves a circle's numeric id to its rgcode — same rationale as `get_tag_name_by_id`.
pub fn get_circle_rgcode_by_id(conn: &Connection, cir_id: i64) -> Result<Option<String>, HvtError> {
    let rgcode: Option<String> = conn
        .query_row(
            &format!("SELECT rgcode FROM {DB_CIRCLE_NAME} WHERE cir_id = ?1"),
            params![cir_id],
            |row| row.get(0),
        )
        .ok();
    Ok(rgcode)
}

/// Resolves a circle's display name (custom preference applied) from its rgcode — used to
/// render a human-readable "Filtered by: Circle: <name>" label on `/works`, since the URL only
/// carries the stable rgcode, not a display name.
pub fn get_circle_display_name_by_rgcode(conn: &Connection, rgcode: &str) -> Result<Option<String>, HvtError> {
    let sql = format!(
        "SELECT {circle_expr} FROM {DB_CIRCLE_NAME} c
         LEFT JOIN {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} ccm ON ccm.cir_id = c.cir_id
         WHERE c.rgcode = ?1",
        circle_expr = merged_circle_name_expr(),
    );
    Ok(conn.query_row(&sql, params![rgcode], |row| row.get(0)).ok())
}

/// Total number of active (non-trashed) works — for the stats page.
pub fn count_all_active_works(conn: &Connection) -> Result<i64, HvtError> {
    Ok(conn.query_row(
        &format!("SELECT COUNT(*) FROM {DB_FOLDERS_NAME} WHERE active = 1"),
        [],
        |row| row.get(0),
    )?)
}

/// Top `limit` tags by active-work count, grouped by merged/display name (two DLSite tags
/// custom-renamed to the same display name count together), excluding ignored tags.
pub fn top_tags_by_count(conn: &Connection, limit: i64) -> Result<Vec<(String, i64)>, HvtError> {
    let sql = format!(
        "SELECT COALESCE(ctm.custom_tag_name, dt.tag_name) AS display_name,
                COUNT(DISTINCT lwt.fld_id) AS work_count
         FROM {DB_LKP_WORK_TAG_NAME} lwt
         JOIN {DB_DLSITE_TAG_NAME} dt ON dt.tag_id = lwt.tag_id
         JOIN {DB_FOLDERS_NAME} f ON f.fld_id = lwt.fld_id AND f.active = 1
         LEFT JOIN {DB_CUSTOM_TAG_MAPPINGS_NAME} ctm ON ctm.dlsite_tag_id = dt.tag_id
         WHERE COALESCE(ctm.is_ignored, 0) = 0
         GROUP BY display_name
         ORDER BY work_count DESC, display_name COLLATE NOCASE ASC
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Top `limit` circles by active-work count. Returns `(rgcode, display_name, work_count)`,
/// grouped by `cir_id` (not display name — two circles could coincidentally share a custom
/// display name, but rgcode is the stable per-circle key used everywhere else).
pub fn top_circles_by_count(conn: &Connection, limit: i64) -> Result<Vec<(String, String, i64)>, HvtError> {
    let sql = format!(
        "SELECT c.rgcode, {circle_expr} AS display_name, COUNT(DISTINCT lwc.fld_id) AS work_count
         FROM {DB_LKP_WORK_CIRCLE_NAME} lwc
         JOIN {DB_CIRCLE_NAME} c ON c.cir_id = lwc.cir_id
         JOIN {DB_FOLDERS_NAME} f ON f.fld_id = lwc.fld_id AND f.active = 1
         LEFT JOIN {DB_CUSTOM_CIRCLE_MAPPINGS_NAME} ccm ON ccm.cir_id = c.cir_id
         GROUP BY c.cir_id
         ORDER BY work_count DESC, display_name COLLATE NOCASE ASC
         LIMIT ?1",
        circle_expr = merged_circle_name_expr(),
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Top `limit` voice actors by active-work count, grouped by merged/display name (a custom
/// rename in `custom_cv_mappings` collapses near-duplicate DLSite entries into one count).
/// Returns `(display_name, work_count)`.
pub fn top_cvs_by_count(conn: &Connection, limit: i64) -> Result<Vec<(String, i64)>, HvtError> {
    let sql = format!(
        "SELECT COALESCE(ccvm.custom_name, cv.name_jp) AS display_name,
                COUNT(DISTINCT lwcv.fld_id) AS work_count
         FROM {DB_LKP_WORK_CVS_NAME} lwcv
         JOIN {DB_CVS_NAME} cv ON cv.cv_id = lwcv.cv_id
         JOIN {DB_FOLDERS_NAME} f ON f.fld_id = lwcv.fld_id AND f.active = 1
         LEFT JOIN {DB_CUSTOM_CV_MAPPINGS_NAME} ccvm ON ccvm.cv_id = cv.cv_id
         GROUP BY display_name
         ORDER BY work_count DESC, display_name COLLATE NOCASE ASC
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Resolves a CV's numeric id to its name_jp — mutation routes take an id (not a name) to
/// avoid URL-encoding arbitrary Japanese text, then bridge back to the `&str`-based
/// `custom_cvs` functions via this lookup. Same rationale as `get_tag_name_by_id`.
pub fn get_cv_name_by_id(conn: &Connection, cv_id: i64) -> Result<Option<String>, HvtError> {
    let name: Option<String> = conn
        .query_row(
            &format!("SELECT name_jp FROM {DB_CVS_NAME} WHERE cv_id = ?1"),
            params![cv_id],
            |row| row.get(0),
        )
        .ok();
    Ok(name)
}

/// Marks a work inactive and records its new (post-move) path in one statement. Call ONLY after
/// the folder has already been physically moved — never before, to avoid a DB-says-trashed but
/// files-untouched inconsistency if the move fails. Deliberately touches only `folders`; every
/// child row (tags/circle/cv/rating/stars/release_date) is left intact so the work stays fully
/// restorable by moving the folder back and flipping `active` to 1 by hand.
pub fn deactivate_and_relocate_work(conn: &Connection, rjcode: &RJCode, new_path: &str) -> Result<(), HvtError> {
    conn.execute(
        &format!("UPDATE {DB_FOLDERS_NAME} SET active = 0, path = ?1 WHERE rjcode = ?2"),
        params![new_path, rjcode.as_str()],
    )?;
    Ok(())
}
