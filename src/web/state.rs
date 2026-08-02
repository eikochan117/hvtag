use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::config::Config;
use crate::web::jobs::JobManager;

/// Shared state for all web UI handlers. `Connection` is `Send` but not `Sync`, and axum
/// handlers run concurrently across tokio tasks, so it's wrapped in a mutex. Every handler's
/// DB access is a quick synchronous local SQLite call that never spans an `.await`, so a plain
/// `std::sync::Mutex` (not `tokio::sync::Mutex`, not a connection pool) is the right amount of
/// machinery here.
///
/// The import job (`jobs`) deliberately does *not* share this connection: it runs for minutes
/// at a time and needs `&Connection` across many `.await` points, so it opens its own SQLite
/// connection for the duration of the job instead of holding this mutex the whole time.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    /// Filesystem path of the database behind `db` — the import job opens its own connection to
    /// this same path (see `web::jobs::JobManager::start_import`) rather than sharing `db`.
    pub db_path: String,
    pub page_size: i64,
    pub config: Config,
    pub jobs: JobManager,
}
