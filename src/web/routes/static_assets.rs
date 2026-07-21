use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::database::web_queries;
use crate::web::state::AppState;

const HTMX_JS: &str = include_str!("../../../static/vendor/htmx.min.js");

const PLACEHOLDER_COVER_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="480" viewBox="0 0 480 480">
<rect width="480" height="480" fill="#2a2a2a"/>
<text x="240" y="240" font-family="sans-serif" font-size="22" fill="#888" text-anchor="middle" dominant-baseline="middle">No cover</text>
</svg>"##;

/// GET /static/htmx.min.js — vendored, embedded at compile time (no CDN dependency, since the
/// phone connects only over VPN and may have no general internet route).
pub async fn htmx_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], HTMX_JS)
}

/// GET /covers/{rjcode} — serves `<folder_path>/folder.jpeg`, or an inline SVG placeholder if
/// the work has no cover yet. Never 404s, so `<img>` tags never show a broken-image icon.
pub async fn cover_image(State(state): State<AppState>, Path(rjcode): Path<String>) -> Response {
    let folder_path = {
        let conn = state.db.lock().expect("db mutex poisoned");
        web_queries::get_folder_path(&conn, &rjcode).ok().flatten()
    };

    if let Some(folder_path) = folder_path {
        let cover_path = std::path::Path::new(&folder_path).join("folder.jpeg");
        if let Ok(bytes) = std::fs::read(&cover_path) {
            return ([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response();
        }
    }

    ([(header::CONTENT_TYPE, "image/svg+xml")], PLACEHOLDER_COVER_SVG).into_response()
}
