//! Runs a single background import job at a time and exposes its live event stream to web
//! handlers. The actual event/question wire format and the `InteractionProvider`/`ProgressSink`
//! bridge live in `crate::interaction::web_provider` — this module only owns "is a job running,
//! and if so, which channel is it talking on".

use std::sync::Arc;

use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;

use crate::config::Config;
use crate::interaction::web_provider::{JobChannel, WebInteractionProvider, WebProgressSink};

#[derive(Clone)]
pub struct JobManager {
    current: Arc<AsyncMutex<Option<Arc<JobChannel>>>>,
}

impl JobManager {
    pub fn new() -> Self {
        JobManager {
            current: Arc::new(AsyncMutex::new(None)),
        }
    }

    /// The channel of the currently running job, if any.
    pub async fn current_channel(&self) -> Option<Arc<JobChannel>> {
        self.current.lock().await.clone()
    }

    /// Whether a job is currently running — lets a freshly loaded/reloaded page decide whether
    /// to reattach to a live job without paying for a WebSocket round trip just to find out.
    pub async fn is_running(&self) -> bool {
        self.current.lock().await.is_some()
    }

    /// Starts the `--full` import workflow as a background task, wired to a fresh `JobChannel`.
    /// Fails if a job is already running — only one import job runs at a time (it, like the
    /// CLI, drives a single global VPN tunnel and a single library on disk).
    ///
    /// `db_path` must be the same database the rest of the web UI (`AppState::db`) is reading
    /// from — the job opens its own connection (see the threading note below) rather than
    /// sharing `AppState::db`, so it has to be told explicitly where that database lives instead
    /// of assuming the default `~/.hvtag/data.db3` path.
    pub async fn start_import(&self, app_config: Config, db_path: String) -> Result<(), &'static str> {
        let mut guard = self.current.lock().await;
        if guard.is_some() {
            return Err("An import job is already running");
        }

        let channel = JobChannel::new();
        *guard = Some(channel.clone());
        drop(guard);

        let manager = self.clone();

        // `rusqlite::Connection` isn't `Sync`, so a `&Connection` held across the workflow's
        // many `.await` points can't cross into `tokio::spawn`'s `Send`-future requirement.
        // Rather than thread `&mut Connection` (or a lock-per-query) through the entire
        // tagging/import pipeline just to satisfy that bound, the job runs on its own OS thread
        // with its own single-threaded Tokio runtime — nothing here ever needs to move across
        // worker threads, so `Send` doesn't apply. The `tokio::sync` channels in `JobChannel`
        // and the app's shared `AsyncMutex` are runtime-agnostic and work fine across runtimes.
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    channel.finish(false, format!("Failed to start job runtime: {}", e));
                    return;
                }
            };

            rt.block_on(async move {
                let progress = WebProgressSink::new(channel.clone());
                let interaction = WebInteractionProvider::new(channel.clone());

                let result = match crate::database::db_loader::open_db(Some(&db_path)) {
                    Ok(db) => {
                        crate::workflows::import::run_import_workflow(&db, &app_config, &progress, &interaction)
                            .await
                    }
                    Err(e) => Err(e),
                };

                let (ok, message) = match result {
                    Ok(()) => (true, "Import job finished".to_string()),
                    Err(e) => {
                        warn!("Import job failed: {}", e);
                        (false, format!("Import job failed: {}", e))
                    }
                };
                channel.finish(ok, message);

                // Free the slot so a new job can be started. Existing WS subscribers keep their
                // own `Arc<JobChannel>` clone and can keep draining buffered events after this.
                *manager.current.lock().await = None;
            });
        });

        Ok(())
    }
}
