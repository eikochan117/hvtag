//! Abstraction over the human-in-the-loop decisions needed while tagging a work.
//!
//! The tagging pipeline sometimes needs a human to disambiguate something it can't infer
//! automatically (e.g. how to parse track numbers out of a folder's filenames). The pipeline
//! itself stays frontend-agnostic: it talks to an `InteractionProvider` and doesn't know or
//! care whether the other end is a terminal (`CliInteractionProvider`) or, eventually, a
//! browser tab waiting on a WebSocket message.

pub mod cli;
pub mod progress;
pub mod web_provider;

use async_trait::async_trait;
use serde::Serialize;

use crate::errors::HvtError;
use crate::tagger::track_parser::TrackParsingPreference;

/// A strategy choice offered to the user when automatic track-number parsing is unreliable.
#[derive(Debug, Clone)]
pub enum TrackStrategyChoice {
    Preference(TrackParsingPreference),
    Manual,
    Skip,
}

/// One row of a parsing preview: a filename and the track number (if any) a candidate
/// strategy would assign to it.
#[derive(Debug, Clone, Serialize)]
pub struct TrackPreviewRow {
    pub filename: String,
    pub track_number: Option<u32>,
}

/// Labels for the track-parsing strategy menu, in the same order `pick_strategy_by_index`
/// expects. Shared by every `InteractionProvider` so the CLI menu and the web menu never
/// drift apart.
pub const STRATEGY_MENU_OPTIONS: [&str; 8] = [
    "Asian full-width numbers  (０１２ → 012)",
    "Asian brackets            【01】 ［01］ 〔01〕 （01）",
    "Kanji episode markers     第01話  第01章  第01回",
    "Custom delimiter          (number followed by a pattern)",
    "Strip prefix then first number  (regex, e.g. s.*?_ strips s19_ from s19_01_track)",
    "First number in filename  (fallback)",
    "Manual numbering          (enter each track number by hand)",
    "Skip this folder          (no track numbers)",
];

/// Outcome of resolving a menu selection: the strategy choice to use, plus an optional
/// user-facing warning (e.g. an invalid regex falling back to a default strategy).
pub struct StrategyResolution {
    pub choice: TrackStrategyChoice,
    pub warning: Option<String>,
}

/// Resolves a menu selection (see `STRATEGY_MENU_OPTIONS`) into a strategy choice.
///
/// Indices 3 (custom delimiter) and 4 (strip-prefix regex) need one more piece of input from
/// the user before a `TrackParsingPreference` can be built — callers fetch those beforehand
/// (via `input_delimiter`/`input_regex_pattern`) and pass them in here, since fetching them is
/// async and this resolution step is pure/sync.
pub fn strategy_choice_from_index(
    index: usize,
    delimiter: Option<String>,
    regex_pattern: Option<String>,
) -> StrategyResolution {
    let pref = |strategy_name: &str, use_asian_conversion: bool, asian_format_type: Option<&str>| {
        TrackParsingPreference {
            strategy_name: strategy_name.to_string(),
            custom_delimiter: None,
            use_asian_conversion,
            asian_format_type: asian_format_type.map(|s| s.to_string()),
            strip_prefix_pattern: None,
        }
    };
    let no_warning = |choice| StrategyResolution { choice, warning: None };

    match index {
        0 => no_warning(TrackStrategyChoice::Preference(pref("asian_fullwidth", true, Some("fullwidth")))),
        1 => no_warning(TrackStrategyChoice::Preference(pref("asian_brackets", true, Some("asian_brackets")))),
        2 => no_warning(TrackStrategyChoice::Preference(pref("asian_kanji_episode", true, Some("kanji_episode")))),
        3 => no_warning(TrackStrategyChoice::Preference(TrackParsingPreference {
            strategy_name: "custom_delimiter".to_string(),
            custom_delimiter: Some(delimiter.unwrap_or_default()),
            use_asian_conversion: false,
            asian_format_type: None,
            strip_prefix_pattern: None,
        })),
        4 => {
            let pattern = regex_pattern.unwrap_or_default();
            match regex::Regex::new(&pattern) {
                Ok(_) => no_warning(TrackStrategyChoice::Preference(TrackParsingPreference {
                    strategy_name: "strip_prefix".to_string(),
                    custom_delimiter: None,
                    use_asian_conversion: false,
                    asian_format_type: None,
                    strip_prefix_pattern: Some(pattern),
                })),
                Err(e) => StrategyResolution {
                    choice: TrackStrategyChoice::Preference(pref("first_number", false, None)),
                    warning: Some(format!(
                        "Invalid regex: {}. Falling back to first-number strategy.",
                        e
                    )),
                },
            }
        }
        5 => no_warning(TrackStrategyChoice::Preference(pref("first_number", false, None))),
        6 => no_warning(TrackStrategyChoice::Manual),
        _ => no_warning(TrackStrategyChoice::Skip),
    }
}

#[async_trait]
pub trait InteractionProvider: Send + Sync {
    /// Show the work's file list and ask which track-numbering strategy to use.
    async fn pick_track_strategy(
        &self,
        rjcode: &str,
        filenames: &[String],
    ) -> Result<TrackStrategyChoice, HvtError>;

    /// Ask for a custom delimiter string (used by the "custom delimiter" strategy).
    async fn input_delimiter(&self) -> Result<String, HvtError>;

    /// Ask for a regex pattern to strip from filenames (used by the "strip prefix" strategy).
    async fn input_regex_pattern(&self) -> Result<String, HvtError>;

    /// Show a preview of what a candidate strategy would produce and ask for confirmation.
    /// `duplicates` lists track numbers that would be assigned to more than one file.
    async fn confirm_strategy_preview(
        &self,
        preview: &[TrackPreviewRow],
        duplicates: &[u32],
    ) -> Result<bool, HvtError>;

    /// Ask for a manual track number for a single file (`None` = no track number).
    async fn input_manual_track_number(&self, filename: &str) -> Result<Option<u32>, HvtError>;

    /// Surface an informational message tied to the current interaction (e.g. "strategy
    /// rejected, pick another one"). Purely for user feedback — never blocks.
    async fn notify(&self, message: &str);
}
