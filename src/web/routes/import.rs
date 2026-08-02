use askama::Template;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Json};
use futures_util::SinkExt;
use serde_json::json;
use tokio::sync::broadcast;
use tracing::debug;

use crate::interaction::web_provider::{AnswerMessage, JobEvent};
use crate::web::error::AppResult;
use crate::web::state::AppState;

const NOT_CONFIGURED: &str = "(non configuré)";

#[derive(Template)]
#[template(path = "import.html")]
struct ImportTemplate {
    source_path: String,
    library_path: String,
}

/// GET /import — the import job page: start button, live console/progress, interactive
/// questions. All driven client-side over `/import/ws`.
pub async fn import_page(State(state): State<AppState>) -> AppResult<Html<String>> {
    let source_path = state
        .config
        .import
        .source_path
        .clone()
        .unwrap_or_else(|| NOT_CONFIGURED.to_string());
    let library_path = state
        .config
        .import
        .library_path
        .clone()
        .unwrap_or_else(|| NOT_CONFIGURED.to_string());

    Ok(Html(ImportTemplate { source_path, library_path }.render()?))
}

/// GET /import/status — lets the page check on load whether a job is already running (e.g.
/// after a reload mid-import) without opening a WebSocket just to find out.
pub async fn import_status(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({ "running": state.jobs.is_running().await }))
}

/// `POST /import/start` — starts the `--full` import workflow as a background job. 409s if a
/// job is already running (only one at a time, same as the CLI).
pub async fn start_import(State(state): State<AppState>) -> impl IntoResponse {
    match state.jobs.start_import(state.config.clone(), state.db_path.clone()).await {
        Ok(()) => (StatusCode::ACCEPTED, "Import job started").into_response(),
        Err(msg) => (StatusCode::CONFLICT, msg).into_response(),
    }
}

/// `GET /import/ws` — live event stream for the currently running import job, and the channel
/// through which the browser answers interactive questions (track-parsing strategy, etc).
///
/// If no job is running when the socket connects, it receives a single explanatory event and
/// the socket closes — the browser is expected to `POST /import/start` first.
pub async fn import_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let Some(channel) = state.jobs.current_channel().await else {
        let event = JobEvent::Log {
            message: "No import job is currently running".to_string(),
        };
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = socket.send(Message::text(json)).await;
        }
        let _ = socket.close().await;
        return;
    };

    let mut events = channel.subscribe();

    loop {
        tokio::select! {
            event = events.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        debug!("import job WS subscriber lagged, skipped {} event(s)", skipped);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                };

                let is_finished = matches!(event, JobEvent::Finished { .. });
                let Ok(json) = serde_json::to_string(&event) else { continue };
                if socket.send(Message::text(json)).await.is_err() {
                    break;
                }
                if is_finished {
                    break;
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<AnswerMessage>(&text) {
                            Ok(msg) => {
                                if let Err(e) = channel.answer(msg.id, msg.answer) {
                                    debug!("import job WS answer rejected: {}", e);
                                }
                            }
                            Err(e) => debug!("import job WS: malformed answer message: {}", e),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}
