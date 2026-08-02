//! Web implementation of `InteractionProvider`/`ProgressSink`, backed by a broadcast channel
//! of JSON-serializable events (consumed by the web UI's WebSocket route) and a one-question-
//! at-a-time answer slot resolved by that same route when the browser responds.
//!
//! This module only defines the plumbing and wire format; the actual Axum route lives in
//! `crate::web::jobs`, which owns a `JobChannel` for the lifetime of a running job.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, oneshot};

use crate::errors::HvtError;
use crate::interaction::progress::ProgressSink;
use crate::interaction::{
    strategy_choice_from_index, InteractionProvider, TrackPreviewRow, TrackStrategyChoice,
    STRATEGY_MENU_OPTIONS,
};

/// Server -> client. Either a status update or a question that pauses the job until answered.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobEvent {
    Phase { name: String },
    StepStart { total: u64 },
    Item { message: String },
    StepFinish,
    Log { message: String },
    Question { id: u64, question: JobQuestion },
    Finished { ok: bool, message: String },
}

/// The payload of a `JobEvent::Question`, one per `InteractionProvider` decision point.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobQuestion {
    TrackStrategyMenu {
        rjcode: String,
        filenames: Vec<String>,
        options: Vec<String>,
    },
    Delimiter,
    RegexPattern,
    StrategyPreview {
        preview: Vec<TrackPreviewRow>,
        duplicates: Vec<u32>,
    },
    ManualTrackNumber {
        filename: String,
    },
}

/// Client -> server. Must match the shape expected by the `JobQuestion` currently pending.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobAnswer {
    Index { index: usize },
    Text { value: String },
    Confirm { value: bool },
    OptionalNumber { value: Option<u32> },
}

/// Envelope the client sends over the `/import/ws` socket to answer a pending question — `id`
/// must match the id from the `JobEvent::Question` being answered.
#[derive(Debug, Deserialize)]
pub struct AnswerMessage {
    pub id: u64,
    pub answer: JobAnswer,
}

/// Bridges a running job to its web frontend: a broadcast stream of `JobEvent`s out, and a
/// single pending-question slot for answers back in. One `JobChannel` per job run.
pub struct JobChannel {
    events_tx: broadcast::Sender<JobEvent>,
    pending: StdMutex<Option<(u64, oneshot::Sender<JobAnswer>)>>,
    next_id: AtomicU64,
}

impl JobChannel {
    pub fn new() -> Arc<Self> {
        let (events_tx, _rx) = broadcast::channel(256);
        Arc::new(JobChannel {
            events_tx,
            pending: StdMutex::new(None),
            next_id: AtomicU64::new(1),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.events_tx.subscribe()
    }

    /// Announces the job's terminal outcome. Called once, after the workflow future resolves.
    pub fn finish(&self, ok: bool, message: String) {
        self.emit(JobEvent::Finished { ok, message });
    }

    fn emit(&self, event: JobEvent) {
        // No receivers (e.g. browser tab not open yet) is not an error — the job keeps running.
        let _ = self.events_tx.send(event);
    }

    /// Registers a question as pending and waits (async, non-blocking) for a matching answer.
    async fn ask(&self, question: JobQuestion) -> Result<JobAnswer, HvtError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut guard = self.pending.lock().unwrap();
            *guard = Some((id, tx));
        }
        self.emit(JobEvent::Question { id, question });
        rx.await
            .map_err(|_| HvtError::Parse("Job was cancelled while waiting for an answer".to_string()))
    }

    /// Resolves the currently pending question if `id` matches. Called by the WS route when the
    /// browser answers.
    pub fn answer(&self, id: u64, answer: JobAnswer) -> Result<(), &'static str> {
        let mut guard = self.pending.lock().unwrap();
        match guard.take() {
            Some((pending_id, tx)) if pending_id == id => {
                let _ = tx.send(answer);
                Ok(())
            }
            Some(other) => {
                *guard = Some(other);
                Err("answer id does not match the currently pending question")
            }
            None => Err("no question is currently pending"),
        }
    }
}

pub struct WebProgressSink {
    channel: Arc<JobChannel>,
}

impl WebProgressSink {
    pub fn new(channel: Arc<JobChannel>) -> Self {
        WebProgressSink { channel }
    }
}

impl ProgressSink for WebProgressSink {
    fn phase(&self, name: &str) {
        self.channel.emit(JobEvent::Phase { name: name.to_string() });
    }

    fn start_step(&self, total: u64) {
        self.channel.emit(JobEvent::StepStart { total });
    }

    fn item(&self, message: &str) {
        self.channel.emit(JobEvent::Item { message: message.to_string() });
    }

    fn finish_step(&self) {
        self.channel.emit(JobEvent::StepFinish);
    }

    fn log(&self, message: &str) {
        self.channel.emit(JobEvent::Log { message: message.to_string() });
    }
}

pub struct WebInteractionProvider {
    channel: Arc<JobChannel>,
}

impl WebInteractionProvider {
    pub fn new(channel: Arc<JobChannel>) -> Self {
        WebInteractionProvider { channel }
    }
}

#[async_trait]
impl InteractionProvider for WebInteractionProvider {
    async fn pick_track_strategy(
        &self,
        rjcode: &str,
        filenames: &[String],
    ) -> Result<TrackStrategyChoice, HvtError> {
        let question = JobQuestion::TrackStrategyMenu {
            rjcode: rjcode.to_string(),
            filenames: filenames.to_vec(),
            options: STRATEGY_MENU_OPTIONS.iter().map(|s| s.to_string()).collect(),
        };
        let index = match self.channel.ask(question).await? {
            JobAnswer::Index { index } => index,
            _ => {
                return Err(HvtError::Parse(
                    "expected an index answer for the strategy menu".to_string(),
                ))
            }
        };

        let delimiter = if index == 3 {
            Some(self.input_delimiter().await?)
        } else {
            None
        };
        let regex_pattern = if index == 4 {
            Some(self.input_regex_pattern().await?)
        } else {
            None
        };

        let resolution = strategy_choice_from_index(index, delimiter, regex_pattern);
        if let Some(warning) = resolution.warning {
            self.notify(&warning).await;
        }
        Ok(resolution.choice)
    }

    async fn input_delimiter(&self) -> Result<String, HvtError> {
        match self.channel.ask(JobQuestion::Delimiter).await? {
            JobAnswer::Text { value } => Ok(value),
            _ => Err(HvtError::Parse(
                "expected a text answer for the delimiter question".to_string(),
            )),
        }
    }

    async fn input_regex_pattern(&self) -> Result<String, HvtError> {
        match self.channel.ask(JobQuestion::RegexPattern).await? {
            JobAnswer::Text { value } => Ok(value),
            _ => Err(HvtError::Parse(
                "expected a text answer for the regex pattern question".to_string(),
            )),
        }
    }

    async fn confirm_strategy_preview(
        &self,
        preview: &[TrackPreviewRow],
        duplicates: &[u32],
    ) -> Result<bool, HvtError> {
        let question = JobQuestion::StrategyPreview {
            preview: preview.to_vec(),
            duplicates: duplicates.to_vec(),
        };
        match self.channel.ask(question).await? {
            JobAnswer::Confirm { value } => Ok(value),
            _ => Err(HvtError::Parse(
                "expected a confirm answer for the strategy preview".to_string(),
            )),
        }
    }

    async fn input_manual_track_number(&self, filename: &str) -> Result<Option<u32>, HvtError> {
        let question = JobQuestion::ManualTrackNumber {
            filename: filename.to_string(),
        };
        match self.channel.ask(question).await? {
            JobAnswer::OptionalNumber { value } => Ok(value),
            _ => Err(HvtError::Parse(
                "expected a number answer for manual track numbering".to_string(),
            )),
        }
    }

    async fn notify(&self, message: &str) {
        self.channel.emit(JobEvent::Log { message: message.to_string() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the core new plumbing end to end without a live server or a real import run:
    /// a "job" task asks a question through `WebInteractionProvider`, a "browser" task drains
    /// the event stream for the matching `Question` event and answers it, and the job task must
    /// resolve with that answer.
    #[tokio::test]
    async fn question_answer_round_trip() {
        let channel = JobChannel::new();
        let mut events = channel.subscribe();

        let provider = WebInteractionProvider::new(channel.clone());
        let job = tokio::spawn(async move { provider.input_manual_track_number("01 - track.mp3").await });

        let event = events.recv().await.expect("expected a Question event");
        let (id, question) = match event {
            JobEvent::Question { id, question } => (id, question),
            other => panic!("expected JobEvent::Question, got {:?}", other),
        };
        match question {
            JobQuestion::ManualTrackNumber { filename } => {
                assert_eq!(filename, "01 - track.mp3");
            }
            other => panic!("expected ManualTrackNumber question, got {:?}", other),
        }

        channel
            .answer(id, JobAnswer::OptionalNumber { value: Some(7) })
            .expect("answer should be accepted");

        let result = job.await.expect("job task panicked").expect("provider call failed");
        assert_eq!(result, Some(7));
    }

    /// An answer for a stale/unknown question id must be rejected rather than silently applied
    /// to whatever question happens to be pending.
    #[tokio::test]
    async fn mismatched_answer_id_is_rejected() {
        let channel = JobChannel::new();
        let mut events = channel.subscribe();

        let provider = WebInteractionProvider::new(channel.clone());
        let job = tokio::spawn(async move { provider.input_delimiter().await });

        let event = events.recv().await.expect("expected a Question event");
        let JobEvent::Question { id, .. } = event else {
            panic!("expected JobEvent::Question");
        };

        let stale_id = id + 1;
        let err = channel
            .answer(stale_id, JobAnswer::Text { value: "_".to_string() })
            .expect_err("mismatched id must be rejected");
        assert_eq!(err, "answer id does not match the currently pending question");

        // The real answer still resolves the still-pending question afterwards.
        channel
            .answer(id, JobAnswer::Text { value: "_".to_string() })
            .expect("correct id should be accepted");
        let result = job.await.expect("job task panicked").expect("provider call failed");
        assert_eq!(result, "_");
    }
}
