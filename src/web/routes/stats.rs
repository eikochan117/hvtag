use askama::Template;
use axum::extract::State;
use axum::response::Html;

use crate::database::web_queries;
use crate::web::error::AppResult;
use crate::web::state::AppState;

const TOP_N: i64 = 10;

/// A single "label (link key) — count" row, reused across the tags/circles/cvs sections.
struct CountRow {
    label: String,
    key: String,
    count: i64,
}

#[derive(Template)]
#[template(path = "stats.html")]
struct StatsTemplate {
    total_works: i64,
    top_tags: Vec<CountRow>,
    top_circles: Vec<CountRow>,
    top_cvs: Vec<CountRow>,
}

/// GET /stats — aggregate counts scoped to active (non-trashed) works, grouped by merged
/// display name where a merge concept exists (tags), by stable id otherwise (circles by
/// rgcode, CVs by name_jp).
pub async fn stats_page(State(state): State<AppState>) -> AppResult<Html<String>> {
    let (total_works, top_tags, top_circles, top_cvs) = {
        let conn = state.db.lock().expect("db mutex poisoned");

        let total_works = web_queries::count_all_active_works(&conn)?;

        let top_tags = web_queries::top_tags_by_count(&conn, TOP_N)?
            .into_iter()
            .map(|(name, count)| CountRow {
                key: name.clone(),
                label: name,
                count,
            })
            .collect();

        let top_circles = web_queries::top_circles_by_count(&conn, TOP_N)?
            .into_iter()
            .map(|(rgcode, display_name, count)| CountRow {
                key: rgcode,
                label: display_name,
                count,
            })
            .collect();

        let top_cvs = web_queries::top_cvs_by_count(&conn, TOP_N)?
            .into_iter()
            .map(|(name_jp, count)| CountRow {
                key: name_jp.clone(),
                label: name_jp,
                count,
            })
            .collect();

        (total_works, top_tags, top_circles, top_cvs)
    };

    Ok(Html(
        StatsTemplate {
            total_works,
            top_tags,
            top_circles,
            top_cvs,
        }
        .render()?,
    ))
}
