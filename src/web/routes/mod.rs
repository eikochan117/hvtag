pub mod circles;
pub mod cvs;
pub mod stats;
pub mod static_assets;
pub mod tags;
pub mod works;

use axum::response::Redirect;
use axum::routing::{get, post};
use axum::Router;

use crate::web::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { Redirect::to("/works") }))
        .route("/works", get(works::works_list_page))
        .route("/works/search", get(works::works_search_partial))
        .route("/works/{rjcode}", get(works::work_detail_page))
        .route("/works/{rjcode}/trash", post(works::trash_work))
        .route("/works/{rjcode}/delete", post(works::delete_work))
        .route("/cvs", get(cvs::cvs_page))
        .route("/cvs/table", get(cvs::cvs_table_partial))
        .route("/cvs/{cv_id}/rename", post(cvs::rename_cv))
        .route("/cvs/{cv_id}/reset", post(cvs::reset_cv))
        .route("/stats", get(stats::stats_page))
        .route("/tags", get(tags::tags_page))
        .route("/tags/table", get(tags::tags_table_partial))
        .route("/tags/{tag_id}/rename", post(tags::rename_tag))
        .route("/tags/{tag_id}/ignore", post(tags::ignore_tag))
        .route("/tags/{tag_id}/reset", post(tags::reset_tag))
        .route("/circles", get(circles::circles_page))
        .route("/circles/table", get(circles::circles_table_partial))
        .route("/circles/{cir_id}/preference", post(circles::set_preference))
        .route("/circles/{cir_id}/reset", post(circles::reset_preference))
        .route("/covers/{rjcode}", get(static_assets::cover_image))
        .route("/static/htmx.min.js", get(static_assets::htmx_js))
        .with_state(state)
}
