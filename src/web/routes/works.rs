use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use crate::database::web_queries::{self, WorkFilter, WorkSort, WorkSummary};
use crate::folders::types::RJCode;
use crate::web::error::AppResult;
use crate::web::state::AppState;

#[derive(Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    q: String,
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    circle: Option<String>,
    #[serde(default)]
    cv: Option<String>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    view: Option<String>,
}

fn default_page() -> i64 {
    1
}

/// "grid" (default: cover-card grid) or "table" (compact data table) — see `works_results.html`.
fn view_param(view: &Option<String>) -> &'static str {
    match view.as_deref() {
        Some("table") => "table",
        _ => "grid",
    }
}

/// Builds the query-layer filter from raw params, treating an empty string the same as
/// "absent". This matters because the hidden `#filter-tag`/`#filter-circle`/`#filter-cv` inputs
/// (see `works_list.html`) are always present in every htmx request via `hx-include`, just empty
/// when no filter is active — without this normalization an unfiltered search would silently
/// turn into "tag = ''" (matching nothing).
fn build_filter(params: &SearchParams) -> WorkFilter<'_> {
    WorkFilter {
        q: &params.q,
        tag: params.tag.as_deref().filter(|s| !s.is_empty()),
        circle: params.circle.as_deref().filter(|s| !s.is_empty()),
        cv: params.cv.as_deref().filter(|s| !s.is_empty()),
    }
}

/// Human-readable label for the "active filter" indicator. The URL carries a stable key (not
/// necessarily a display name) — circles in particular need a DB lookup from rgcode.
fn resolve_active_filter_label(state: &AppState, filter: &WorkFilter) -> AppResult<Option<String>> {
    if let Some(tag) = filter.tag {
        return Ok(Some(format!("Tag: {tag}")));
    }
    if let Some(rgcode) = filter.circle {
        let conn = state.db.lock().expect("db mutex poisoned");
        let name = web_queries::get_circle_display_name_by_rgcode(&conn, rgcode)?
            .unwrap_or_else(|| rgcode.to_string());
        return Ok(Some(format!("Circle: {name}")));
    }
    if let Some(cv) = filter.cv {
        return Ok(Some(format!("Voice actor: {cv}")));
    }
    Ok(None)
}

#[derive(Template)]
#[template(path = "works_results.html")]
struct WorksResultsTemplate {
    works: Vec<WorkSummary>,
    q: String,
    page: i64,
    total_pages: i64,
    sort: &'static str,
    view: &'static str,
}

#[derive(Template)]
#[template(path = "works_list.html")]
struct WorksListTemplate {
    q: String,
    results_html: String,
    tag: Option<String>,
    circle: Option<String>,
    cv: Option<String>,
    active_filter: Option<String>,
}

#[derive(Template)]
#[template(path = "work_detail.html")]
struct WorkDetailTemplate {
    work: web_queries::WorkDetail,
}

/// Runs the search + pagination query and renders just the results partial (shared by the
/// full-page load and the htmx live-search endpoint).
fn render_results(state: &AppState, filter: &WorkFilter, page: i64, sort: WorkSort, view: &Option<String>) -> AppResult<String> {
    let page = page.max(1);
    let limit = state.page_size.max(1);
    let offset = (page - 1) * limit;

    let (works, total) = {
        let conn = state.db.lock().expect("db mutex poisoned");
        let works = web_queries::list_work_summaries(&conn, filter, sort, limit, offset)?;
        let total = web_queries::count_work_summaries(&conn, filter)?;
        (works, total)
    };

    let total_pages = ((total as f64) / (limit as f64)).ceil().max(1.0) as i64;

    let html = WorksResultsTemplate {
        works,
        q: filter.q.to_string(),
        page,
        total_pages,
        sort: sort.as_param(),
        view: view_param(view),
    }
    .render()?;

    Ok(html)
}

/// GET /works — full page, server-rendered on first load (no JS required for first paint).
pub async fn works_list_page(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> AppResult<Html<String>> {
    let filter = build_filter(&params);
    let sort = WorkSort::from_param(params.sort.as_deref());
    let results_html = render_results(&state, &filter, params.page, sort, &params.view)?;
    let active_filter = resolve_active_filter_label(&state, &filter)?;

    let html = WorksListTemplate {
        q: params.q,
        results_html,
        tag: params.tag,
        circle: params.circle,
        cv: params.cv,
        active_filter,
    }
    .render()?;
    Ok(Html(html))
}

/// GET /works/search — htmx partial, swapped into #work-results on keyup/pagination/sort/view.
pub async fn works_search_partial(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> AppResult<Html<String>> {
    let filter = build_filter(&params);
    let sort = WorkSort::from_param(params.sort.as_deref());
    let html = render_results(&state, &filter, params.page, sort, &params.view)?;
    Ok(Html(html))
}

/// GET /works/{rjcode} — work detail page.
pub async fn work_detail_page(
    State(state): State<AppState>,
    Path(rjcode): Path<String>,
) -> AppResult<Response> {
    let rjcode = match RJCode::new(rjcode) {
        Ok(code) => code,
        Err(_) => return Ok((StatusCode::NOT_FOUND, "Invalid work code").into_response()),
    };

    let detail = {
        let conn = state.db.lock().expect("db mutex poisoned");
        web_queries::get_work_detail(&conn, &rjcode)?
    };

    let Some(work) = detail else {
        return Ok((StatusCode::NOT_FOUND, "Work not found").into_response());
    };

    let html = WorkDetailTemplate { work }.render()?;
    Ok(Html(html).into_response())
}

/// POST /works/{rjcode}/trash — moves the work's folder to a SIBLING `.trash/<rjcode>` dir
/// (i.e. `<parent-of-folder_path>/.trash/<rjcode>`, not a globally configured trash path — this
/// guarantees the move stays on the same volume/share, so `move_folder_cross_drive` takes its
/// fast-rename path in practice), then flips `folders.active` to 0 and updates `folders.path`.
/// Every listing query already filters on `active = 1`, so this alone removes the work from the
/// UI — no other query changes needed. Not a permanent delete: no child rows are touched, so
/// it's reversible by hand (move the folder back, set `active = 1`). If the move fails, the DB
/// is NOT touched, to avoid a DB-says-trashed-but-files-untouched inconsistent state.
pub async fn trash_work(State(state): State<AppState>, Path(rjcode): Path<String>) -> AppResult<Response> {
    let rjcode = match RJCode::new(rjcode) {
        Ok(code) => code,
        Err(_) => return Ok((StatusCode::NOT_FOUND, "Invalid work code").into_response()),
    };

    let folder_path = {
        let conn = state.db.lock().expect("db mutex poisoned");
        web_queries::get_folder_path(&conn, rjcode.as_str())?
    };
    let Some(folder_path) = folder_path.filter(|p| !p.is_empty()) else {
        return Ok((StatusCode::NOT_FOUND, "Work not found or has no folder path").into_response());
    };

    let source = std::path::PathBuf::from(&folder_path);
    let Some(parent) = source.parent() else {
        return Ok((StatusCode::INTERNAL_SERVER_ERROR, "Folder has no parent directory").into_response());
    };
    let trash_dir = parent.join(".trash");
    let target = trash_dir.join(rjcode.as_str());

    if target.exists() {
        return Ok((StatusCode::CONFLICT, "A .trash entry for this work already exists").into_response());
    }

    std::fs::create_dir_all(&trash_dir)?;
    crate::move_folder_cross_drive(&source, &target)?;

    {
        let conn = state.db.lock().expect("db mutex poisoned");
        web_queries::deactivate_and_relocate_work(&conn, &rjcode, &target.to_string_lossy())?;
    }

    Ok((StatusCode::OK, [("HX-Redirect", "/works")]).into_response())
}

/// POST /works/{rjcode}/delete — permanently removes the work from the database, with NO
/// filesystem interaction at all. For works whose folder is already gone from disk (e.g. deleted
/// outside hvtag), where `trash_work`'s file-move step doesn't apply and would just error out.
/// Unlike trash, this is NOT reversible — every child row (tags/circle/cv links, rating, stars,
/// release_date, covers) is gone for good, not just deactivated.
pub async fn delete_work(State(state): State<AppState>, Path(rjcode): Path<String>) -> AppResult<Response> {
    let rjcode = match RJCode::new(rjcode) {
        Ok(code) => code,
        Err(_) => return Ok((StatusCode::NOT_FOUND, "Invalid work code").into_response()),
    };

    let conn = state.db.lock().expect("db mutex poisoned");
    if !crate::database::queries::rjcode_exists(&conn, &rjcode)? {
        return Ok((StatusCode::NOT_FOUND, "Work not found").into_response());
    }
    crate::database::queries::delete_work_permanently(&conn, &rjcode)?;

    Ok((StatusCode::OK, [("HX-Redirect", "/works")]).into_response())
}
