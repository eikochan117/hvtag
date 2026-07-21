use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use crate::database::custom_tags;
use crate::database::web_queries;
use crate::web::error::AppResult;
use crate::web::state::AppState;

/// Named view of `custom_tags::list_all_dlsite_tags_with_counts`'s tuple, for template ergonomics.
struct TagRow {
    tag_id: i64,
    tag_name: String,
    custom_name: Option<String>,
    is_ignored: bool,
    work_count: i64,
}

impl TagRow {
    /// The exact string `custom_tags::get_merged_tags_for_work` would emit for this tag — used
    /// as the `?tag=` filter value so a click matches the same works the chip would show. For
    /// an ignored tag this correctly yields "no works found" when clicked, since ignored tags
    /// never appear in any work's merged tag set by definition.
    fn display_name(&self) -> &str {
        self.custom_name.as_deref().unwrap_or(&self.tag_name)
    }
}

#[derive(Template)]
#[template(path = "tags_table.html")]
struct TagsTableTemplate {
    tags: Vec<TagRow>,
    sort: String,
    dir: String,
}

#[derive(Template)]
#[template(path = "tags_page.html")]
struct TagsPageTemplate {
    table_html: String,
}

#[derive(Deserialize)]
pub struct RenameForm {
    custom_tag_name: String,
}

/// Column-sort query params for /tags and /tags/table. `sort` is one of "tag" (default),
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
        Some("custom") => "ctm.custom_tag_name",
        Some("works") => "work_count",
        _ => "dt.tag_name",
    };
    format!("{column} {dir}, dt.tag_name COLLATE NOCASE ASC")
}

fn render_table(state: &AppState, params: &SortParams) -> AppResult<String> {
    let conn = state.db.lock().expect("db mutex poisoned");
    let tags = custom_tags::list_all_dlsite_tags_with_counts(&conn, &order_by(params))?
        .into_iter()
        .map(|(tag_id, tag_name, custom_name, is_ignored, work_count)| TagRow {
            tag_id,
            tag_name,
            custom_name,
            is_ignored,
            work_count,
        })
        .collect();

    let template = TagsTableTemplate {
        tags,
        sort: params.sort.clone().unwrap_or_else(|| "tag".to_string()),
        dir: if params.dir.as_deref() == Some("desc") { "desc".to_string() } else { "asc".to_string() },
    };
    Ok(template.render()?)
}

/// GET /tags
pub async fn tags_page(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<SortParams>,
) -> AppResult<Html<String>> {
    let table_html = render_table(&state, &params)?;
    Ok(Html(TagsPageTemplate { table_html }.render()?))
}

/// GET /tags/table — htmx partial, swapped into #tags-table-container on column-header sort.
pub async fn tags_table_partial(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<SortParams>,
) -> AppResult<Html<String>> {
    Ok(Html(render_table(&state, &params)?))
}

fn resolve_tag_name(state: &AppState, tag_id: i64) -> AppResult<Option<String>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    Ok(web_queries::get_tag_name_by_id(&conn, tag_id)?)
}

/// POST /tags/{tag_id}/rename?sort=..&dir=.. — sort/dir carried as query params (not form fields)
/// so the table re-renders with whatever sort was active before the mutation, instead of
/// silently resetting to the default every time a row is edited.
pub async fn rename_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
    axum::extract::Query(sort_params): axum::extract::Query<SortParams>,
    axum::Form(form): axum::Form<RenameForm>,
) -> AppResult<Response> {
    let Some(tag_name) = resolve_tag_name(&state, tag_id)? else {
        return Ok((StatusCode::NOT_FOUND, "Tag not found").into_response());
    };

    let custom_name = form.custom_tag_name.trim();
    if !custom_name.is_empty() {
        let conn = state.db.lock().expect("db mutex poisoned");
        custom_tags::add_custom_tag_mapping(&conn, &tag_name, custom_name)?;
        custom_tags::mark_works_for_retagging(&conn, &tag_name)?;
    }

    Ok(Html(render_table(&state, &sort_params)?).into_response())
}

/// POST /tags/{tag_id}/ignore?sort=..&dir=..
pub async fn ignore_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
    axum::extract::Query(sort_params): axum::extract::Query<SortParams>,
) -> AppResult<Response> {
    let Some(tag_name) = resolve_tag_name(&state, tag_id)? else {
        return Ok((StatusCode::NOT_FOUND, "Tag not found").into_response());
    };

    {
        let conn = state.db.lock().expect("db mutex poisoned");
        custom_tags::ignore_tag(&conn, &tag_name)?;
        custom_tags::mark_works_for_retagging(&conn, &tag_name)?;
    }

    Ok(Html(render_table(&state, &sort_params)?).into_response())
}

/// POST /tags/{tag_id}/reset?sort=..&dir=.. — reverts a rename or un-ignores, back to the
/// DLSite default name.
pub async fn reset_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
    axum::extract::Query(sort_params): axum::extract::Query<SortParams>,
) -> AppResult<Response> {
    let Some(tag_name) = resolve_tag_name(&state, tag_id)? else {
        return Ok((StatusCode::NOT_FOUND, "Tag not found").into_response());
    };

    {
        let conn = state.db.lock().expect("db mutex poisoned");
        custom_tags::remove_custom_tag_mapping(&conn, &tag_name)?;
        custom_tags::mark_works_for_retagging(&conn, &tag_name)?;
    }

    Ok(Html(render_table(&state, &sort_params)?).into_response())
}
