use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use crate::database::custom_circles::{self, CirclePreferenceType};
use crate::database::web_queries;
use crate::web::error::AppResult;
use crate::web::state::AppState;

/// Named view of `custom_circles::list_all_circles`'s tuple, for template ergonomics.
struct CircleRow {
    cir_id: i64,
    rgcode: String,
    name_en: String,
    name_jp: String,
    pref_type: Option<String>,
    custom_name: Option<String>,
}

#[derive(Template)]
#[template(path = "circles_table.html")]
struct CirclesTableTemplate {
    circles: Vec<CircleRow>,
    sort: String,
    dir: String,
}

#[derive(Template)]
#[template(path = "circles_page.html")]
struct CirclesPageTemplate {
    table_html: String,
}

#[derive(Deserialize)]
pub struct PreferenceForm {
    preference_type: String,
    #[serde(default)]
    custom_name: String,
}

/// Column-sort query params for /circles and /circles/table. `sort` is one of "name" (default),
/// "rgcode", "pref"; `dir` is "asc" (default) or "desc". Values are whitelisted below, never
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
    let name_expr = custom_circles::CIRCLE_NAME_EXPR;
    match params.sort.as_deref() {
        Some("rgcode") => format!("c.rgcode {dir}"),
        Some("pref") => format!("ccm.preference_type {dir}, {name_expr} ASC"),
        _ => format!("{name_expr} {dir}"),
    }
}

fn render_table(state: &AppState, params: &SortParams) -> AppResult<String> {
    let conn = state.db.lock().expect("db mutex poisoned");
    let circles = custom_circles::list_all_circles(&conn, &order_by(params))?
        .into_iter()
        .map(|(cir_id, rgcode, name_en, name_jp, pref_type, custom_name)| CircleRow {
            cir_id,
            rgcode,
            name_en,
            name_jp,
            pref_type,
            custom_name,
        })
        .collect();

    let template = CirclesTableTemplate {
        circles,
        sort: params.sort.clone().unwrap_or_else(|| "name".to_string()),
        dir: if params.dir.as_deref() == Some("desc") { "desc".to_string() } else { "asc".to_string() },
    };
    Ok(template.render()?)
}

/// GET /circles
pub async fn circles_page(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<SortParams>,
) -> AppResult<Html<String>> {
    let table_html = render_table(&state, &params)?;
    Ok(Html(CirclesPageTemplate { table_html }.render()?))
}

/// GET /circles/table — htmx partial, swapped into #circles-table-container on column-header sort.
pub async fn circles_table_partial(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<SortParams>,
) -> AppResult<Html<String>> {
    Ok(Html(render_table(&state, &params)?))
}

fn resolve_rgcode(state: &AppState, cir_id: i64) -> AppResult<Option<String>> {
    let conn = state.db.lock().expect("db mutex poisoned");
    Ok(web_queries::get_circle_rgcode_by_id(&conn, cir_id)?)
}

/// POST /circles/{cir_id}/preference?sort=..&dir=.. — sort/dir carried as query params (not form
/// fields) so the table re-renders with whatever sort was active before the mutation.
pub async fn set_preference(
    State(state): State<AppState>,
    Path(cir_id): Path<i64>,
    axum::extract::Query(sort_params): axum::extract::Query<SortParams>,
    axum::Form(form): axum::Form<PreferenceForm>,
) -> AppResult<Response> {
    let Some(rgcode) = resolve_rgcode(&state, cir_id)? else {
        return Ok((StatusCode::NOT_FOUND, "Circle not found").into_response());
    };

    let Some(preference) = CirclePreferenceType::from_str(&form.preference_type) else {
        return Ok((StatusCode::BAD_REQUEST, "Invalid preference type").into_response());
    };

    let custom_name = form.custom_name.trim();
    if preference == CirclePreferenceType::Custom && custom_name.is_empty() {
        return Ok((StatusCode::BAD_REQUEST, "custom_name is required for the custom preference").into_response());
    }

    {
        let conn = state.db.lock().expect("db mutex poisoned");
        let custom_name_opt = if custom_name.is_empty() { None } else { Some(custom_name) };
        custom_circles::set_circle_preference(&conn, &rgcode, preference, custom_name_opt)?;
        custom_circles::mark_circle_works_for_retagging(&conn, &rgcode)?;
    }

    Ok(Html(render_table(&state, &sort_params)?).into_response())
}

/// POST /circles/{cir_id}/reset?sort=..&dir=..
pub async fn reset_preference(
    State(state): State<AppState>,
    Path(cir_id): Path<i64>,
    axum::extract::Query(sort_params): axum::extract::Query<SortParams>,
) -> AppResult<Response> {
    let Some(rgcode) = resolve_rgcode(&state, cir_id)? else {
        return Ok((StatusCode::NOT_FOUND, "Circle not found").into_response());
    };

    {
        let conn = state.db.lock().expect("db mutex poisoned");
        custom_circles::remove_circle_preference(&conn, &rgcode)?;
        custom_circles::mark_circle_works_for_retagging(&conn, &rgcode)?;
    }

    Ok(Html(render_table(&state, &sort_params)?).into_response())
}
