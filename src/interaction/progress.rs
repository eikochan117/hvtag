//! Abstraction over how workflow progress is surfaced to a human.
//!
//! Mirrors the split in this module between `InteractionProvider` (decisions) and
//! `ProgressSink` (status): the tagging/import pipeline reports phases, countable steps and
//! log lines without knowing whether they end up as an `indicatif` progress bar in a terminal
//! or as JSON events pushed over a WebSocket to a browser tab.
//!
//! Every method is synchronous and non-blocking by design (a `println!`, a `ProgressBar` call,
//! or a `broadcast::Sender::send`) so the workflow code can call it inline without `.await`.

pub trait ProgressSink: Send + Sync {
    /// Announces a new named phase (e.g. "Fetching metadata"). Any in-flight step from a
    /// previous phase is implicitly done.
    fn phase(&self, name: &str);

    /// Starts a countable step within the current phase (e.g. iterating over N folders).
    fn start_step(&self, total: u64);

    /// Reports one item processed within the current step, advancing its counter.
    fn item(&self, message: &str);

    /// Ends the current step.
    fn finish_step(&self);

    /// A freeform informational line not tied to a countable step.
    fn log(&self, message: &str);
}
