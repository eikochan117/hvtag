use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use crate::database::custom_cvs;
use crate::database::web_queries;
use crate::web::error::AppResult;
use crate::web::state::AppState;

/// Named view of `custom_cvs::list_all_cvs_with_counts`'s tuple, for template ergonomics.
struct CvRow {
    cv_id: i64,
    name_jp: String,
    name_en: Option<String>,
    custom_name: Option<String>,
    work_count: i64,
}

impl CvRow {
    /// The exact string `custom_cvs::get_merged_cvs_for_work` would emit for this CV — used as
    /// the `?cv=` filter value so a click matches the same works the merged name would show.
    fn display_name(&self) -> &str {
        self.custom_name.as_deref().unwrap_or(&self.name_jp)
    }
}

#[derive(Template)]
#[template(path = "cvs_table.html")]
struct CvsTableTemplate {
    cvs: Vec<CvRow>,
    sort: String,
    dir: String,
}

#[derive(Template)]
#[template(path = "cvs_page.html")]
struct CvsPageTemplate {
    table_html: String,
}

#[derive(Deserialize)]
pub struct RenameForm {
    custom_cv_name: String,
}

/// Column-sort query params for /cvs and /cvs/table. `sort` is one of "jp" (default), "en",
/// "custom", "works"; `dir` is "asc" (default) or "desc". Values are whitelisted below, never
/// interpolated into SQL directly.
#[derive(Deserialize, Default)]
pub struct SortParams {
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    dir: Option<String>,
}

fn order_by(params: &SortParams) -> String {
    let dir = if params.dir.as_deref() == Some("desc") { "DESC" } else { "ASC" };
    let column = match params.sort.as_deref() {
        Some("en") => "cv.name_en",
        Some("custom") => "ccvm.custom_name",
        Some("works") => "work_count",
        _ => "cv.name_jp",
    };
    format!("{column} COLLATE NOCASE {dir}, cv.name_jp COLLATE NOCASE ASC")
}

fn render_table(state: &AppState, params: &SortParams) -> AppResult<String> {
    let conn = state.db.lock().expect("db mutex poisoned");
    let cvs = custom_cvs::list_all_cvs_with_counts(&conn, &order_by(params))?
        .into_iter()
        .map(|(cv_id, name_jp, name_en, custom_name, work_count)| CvRow {
            cv_id,
            name_jp,
            name_en,
            custom_name,
            work_count,
        })
        .collect();

    let template = CvsTableTemplate {
        cvs,
        sort: params.sort.clone().unwrap_or_else(|| "jp".to_string()),
        dir: if params.dir.as_deref() == Some("desc") { "desc".to_string() } else { "asc".to_string() },
    };
    Ok(template.render()?)
}

/// GET /cvs
pub async fn cvs_page(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<SortParams>,
) -> AppResult<Html<String>> {
    let table_html = render_table(&state, &params)?;
    Ok(Html(CvsPageTemplate { table_html }.render()?))
}

/// GET /cvs/table — htmx partial, swapped into #cvs-table-container on column-header sort.
pub async fn cvs_table_partial(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<SortParams>,
) -> AppResult<Html<String>> {
    Ok(Html(render_table(&state, &params)?))
}

fn resolve_cv_name(state: &AppState, cv_id: i64) -> AppResult<Option<String>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    Ok(web_queries::get_cv_name_by_id(&conn, cv_id)?)
}

/// POST /cvs/{cv_id}/rename?sort=..&dir=.. — sort/dir carried as query params (not form fields)
/// so the table re-renders with whatever sort was active before the mutation.
pub async fn rename_cv(
    State(state): State<AppState>,
    Path(cv_id): Path<i64>,
    axum::extract::Query(sort_params): axum::extract::Query<SortParams>,
    axum::Form(form): axum::Form<RenameForm>,
) -> AppResult<Response> {
    let Some(cv_name) = resolve_cv_name(&state, cv_id)? else {
        return Ok((StatusCode::NOT_FOUND, "Voice actor not found").into_response());
    };

    let custom_name = form.custom_cv_name.trim();
    if !custom_name.is_empty() {
        let conn = state.db.lock().expect("db mutex poisoned");
        custom_cvs::add_custom_cv_mapping(&conn, &cv_name, custom_name)?;
        custom_cvs::mark_works_for_retagging(&conn, &cv_name)?;
    }

    Ok(Html(render_table(&state, &sort_params)?).into_response())
}

/// POST /cvs/{cv_id}/reset?sort=..&dir=.. — reverts a rename back to the DLSite default name_jp.
pub async fn reset_cv(
    State(state): State<AppState>,
    Path(cv_id): Path<i64>,
    axum::extract::Query(sort_params): axum::extract::Query<SortParams>,
) -> AppResult<Response> {
    let Some(cv_name) = resolve_cv_name(&state, cv_id)? else {
        return Ok((StatusCode::NOT_FOUND, "Voice actor not found").into_response());
    };

    {
        let conn = state.db.lock().expect("db mutex poisoned");
        custom_cvs::remove_custom_cv_mapping(&conn, &cv_name)?;
        custom_cvs::mark_works_for_retagging(&conn, &cv_name)?;
    }

    Ok(Html(render_table(&state, &sort_params)?).into_response())
}
