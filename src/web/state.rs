use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// Shared state for all web UI handlers. `Connection` is `Send` but not `Sync`, and axum
/// handlers run concurrently across tokio tasks, so it's wrapped in a mutex. Every handler's
/// DB access is a quick synchronous local SQLite call that never spans an `.await`, so a plain
/// `std::sync::Mutex` (not `tokio::sync::Mutex`, not a connection pool) is the right amount of
/// machinery here.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub page_size: i64,
}
