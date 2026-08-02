//! Terminal implementation of `InteractionProvider`, backed by `dialoguer`.
//!
//! `dialoguer` reads stdin synchronously, so every call is run inside `spawn_blocking` —
//! the tagging pipeline runs on the async runtime and must not stall it while waiting on
//! a human at a keyboard.

use std::sync::Mutex;

use async_trait::async_trait;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::errors::HvtError;
use crate::interaction::progress::ProgressSink;
use crate::interaction::{
    strategy_choice_from_index, InteractionProvider, TrackPreviewRow, TrackStrategyChoice,
    STRATEGY_MENU_OPTIONS,
};

pub struct CliInteractionProvider;

impl CliInteractionProvider {
    pub fn new() -> Self {
        CliInteractionProvider
    }
}

fn join_err(e: tokio::task::JoinError) -> HvtError {
    HvtError::Parse(format!("Interactive prompt task panicked: {}", e))
}

#[async_trait]
impl InteractionProvider for CliInteractionProvider {
    async fn pick_track_strategy(
        &self,
        rjcode: &str,
        filenames: &[String],
    ) -> Result<TrackStrategyChoice, HvtError> {
        let rjcode = rjcode.to_string();
        let filenames = filenames.to_vec();
        tokio::task::spawn_blocking(move || cli_pick_track_strategy(&rjcode, &filenames))
            .await
            .map_err(join_err)?
    }

    async fn input_delimiter(&self) -> Result<String, HvtError> {
        tokio::task::spawn_blocking(cli_input_delimiter)
            .await
            .map_err(join_err)?
    }

    async fn input_regex_pattern(&self) -> Result<String, HvtError> {
        tokio::task::spawn_blocking(cli_input_regex_pattern)
            .await
            .map_err(join_err)?
    }

    async fn confirm_strategy_preview(
        &self,
        preview: &[TrackPreviewRow],
        duplicates: &[u32],
    ) -> Result<bool, HvtError> {
        let preview = preview.to_vec();
        let duplicates = duplicates.to_vec();
        tokio::task::spawn_blocking(move || cli_confirm_strategy_preview(&preview, &duplicates))
            .await
            .map_err(join_err)?
    }

    async fn input_manual_track_number(&self, filename: &str) -> Result<Option<u32>, HvtError> {
        let filename = filename.to_string();
        tokio::task::spawn_blocking(move || cli_input_manual_track_number(&filename))
            .await
            .map_err(join_err)?
    }

    async fn notify(&self, message: &str) {
        println!("{}", message);
    }
}

// ---------------------------------------------------------------------------
// Blocking helpers (run inside spawn_blocking)
// ---------------------------------------------------------------------------

fn cli_pick_track_strategy(
    rjcode: &str,
    filenames: &[String],
) -> Result<TrackStrategyChoice, HvtError> {
    println!("\n=== Track Number Parsing ===");
    println!("Work: {}", rjcode);
    println!("\nFiles ({} total):", filenames.len());
    for (i, name) in filenames.iter().take(10).enumerate() {
        println!("  {:>2}. {}", i + 1, name);
    }
    if filenames.len() > 10 {
        println!("  ... and {} more", filenames.len() - 10);
    }
    println!("\nAutomatic track number detection failed. Please choose a strategy.\n");

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Parsing strategy")
        .items(&STRATEGY_MENU_OPTIONS)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let delimiter = if selection == 3 {
        Some(cli_input_delimiter()?)
    } else {
        None
    };
    let regex_pattern = if selection == 4 {
        Some(cli_input_regex_pattern()?)
    } else {
        None
    };

    let resolution = strategy_choice_from_index(selection, delimiter, regex_pattern);
    if let Some(warning) = resolution.warning {
        println!("{}", warning);
    }
    Ok(resolution.choice)
}

fn cli_input_delimiter() -> Result<String, HvtError> {
    Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Delimiter before track numbers (e.g. \"_\", \"No.\")")
        .interact_text()
        .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))
}

fn cli_input_regex_pattern() -> Result<String, HvtError> {
    println!("\nRegex pattern to remove from the start of the filename before");
    println!("looking for the first number.");
    println!("Examples:");
    println!("  s.*?_     strips 's19_' from 's19_01_track'");
    println!("  ^\\[.*?\\]\\s*  strips '[se01] ' from '[se01] track name'");
    println!("  (?i)vol\\d+_  strips 'vol3_' (case-insensitive)");
    println!();
    Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Regex pattern to strip")
        .interact_text()
        .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))
}

fn cli_confirm_strategy_preview(
    preview: &[TrackPreviewRow],
    duplicates: &[u32],
) -> Result<bool, HvtError> {
    println!("\n=== Parsing Preview ===");

    let mut success = 0usize;
    let mut failed = 0usize;

    for row in preview.iter().take(10) {
        match row.track_number {
            Some(n) => {
                println!("  [{:>3}] {}", n, row.filename);
                success += 1;
            }
            None => {
                println!("  [ ??] {}", row.filename);
                failed += 1;
            }
        }
    }

    if preview.len() > 10 {
        println!("  ... and {} more", preview.len() - 10);
        for row in preview.iter().skip(10) {
            if row.track_number.is_some() {
                success += 1;
            } else {
                failed += 1;
            }
        }
    }

    println!("\nParsed: {}/{}", success, preview.len());
    if failed > 0 {
        println!(
            "Warning: {}/{} file(s) could not be parsed — they will be tagged without a track number.",
            failed, preview.len()
        );
    }

    if !duplicates.is_empty() {
        println!(
            "Warning: track number(s) {:?} would be assigned to more than one file — those files would collide.",
            duplicates
        );
    }

    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Use this strategy?")
        .default(duplicates.is_empty())
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))
}

fn cli_input_manual_track_number(filename: &str) -> Result<Option<u32>, HvtError> {
    let input: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(filename)
        .allow_empty(true)
        .interact_text()
        .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))?;

    let n = input
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|&v| v > 0 && v < 1000);
    if !input.trim().is_empty() && n.is_none() {
        println!("  (invalid number, skipping)");
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// ProgressSink implementation
// ---------------------------------------------------------------------------

/// Terminal implementation of `ProgressSink`, backed by `indicatif`.
pub struct CliProgressSink {
    current_bar: Mutex<Option<ProgressBar>>,
}

impl CliProgressSink {
    pub fn new() -> Self {
        CliProgressSink {
            current_bar: Mutex::new(None),
        }
    }

    fn make_bar(len: u64) -> ProgressBar {
        let pb = ProgressBar::new(len);
        pb.set_draw_target(ProgressDrawTarget::stdout());
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );
        pb
    }
}

impl ProgressSink for CliProgressSink {
    fn phase(&self, name: &str) {
        if let Some(pb) = self.current_bar.lock().unwrap().take() {
            pb.finish_and_clear();
        }
        println!("\n--- {} ---", name);
    }

    fn start_step(&self, total: u64) {
        let pb = Self::make_bar(total);
        *self.current_bar.lock().unwrap() = Some(pb);
    }

    fn item(&self, message: &str) {
        let guard = self.current_bar.lock().unwrap();
        if let Some(pb) = guard.as_ref() {
            pb.println(message);
            pb.inc(1);
        } else {
            println!("{}", message);
        }
    }

    fn finish_step(&self) {
        if let Some(pb) = self.current_bar.lock().unwrap().take() {
            pb.finish_and_clear();
        }
    }

    fn log(&self, message: &str) {
        println!("{}", message);
    }
}
