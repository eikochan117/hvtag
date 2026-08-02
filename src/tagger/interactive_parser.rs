use crate::errors::HvtError;
use crate::interaction::{InteractionProvider, TrackPreviewRow, TrackStrategyChoice};
use crate::tagger::track_parser::{
    find_duplicate_track_numbers, parse_track_number_with_preference, TrackParsingPreference,
};

/// Result of a completed interactive parsing session.
pub enum ParsingResult {
    /// An automatic strategy to apply to all files (saveable to DB).
    Strategy(TrackParsingPreference),
    /// Explicit per-file track numbers, indexed by position in the file list.
    /// `None` at a given index means "no track number" for that file.
    Manual(Vec<Option<u32>>),
    /// User chose to skip — files will be tagged without track numbers.
    Skip,
}

/// Runs the full interactive track-parsing session via the given `InteractionProvider`.
///
/// Shows the file list, presents the strategy menu, previews the result,
/// and loops back to the menu if the user rejects the preview.
/// Returns only when the user accepts a result or explicitly skips.
pub async fn run_interactive_parsing(
    provider: &dyn InteractionProvider,
    filenames: &[String],
    rjcode: &str,
) -> Result<ParsingResult, HvtError> {
    loop {
        match provider.pick_track_strategy(rjcode, filenames).await? {
            TrackStrategyChoice::Skip => return Ok(ParsingResult::Skip),

            TrackStrategyChoice::Manual => {
                let numbers = collect_manual_numbers(provider, filenames).await?;
                return Ok(ParsingResult::Manual(numbers));
            }

            TrackStrategyChoice::Preference(pref) => {
                let results = test_strategy(filenames, &pref);
                let preview: Vec<TrackPreviewRow> = filenames
                    .iter()
                    .zip(results.iter())
                    .map(|(filename, track_number)| TrackPreviewRow {
                        filename: filename.clone(),
                        track_number: *track_number,
                    })
                    .collect();
                let duplicates = find_duplicate_track_numbers(&results);

                if provider.confirm_strategy_preview(&preview, &duplicates).await? {
                    return Ok(ParsingResult::Strategy(pref));
                }
                provider
                    .notify("Strategy rejected — please pick another one.")
                    .await;
                // loop continues
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Applies a strategy to all filenames and returns the parsed track numbers.
fn test_strategy(filenames: &[String], preference: &TrackParsingPreference) -> Vec<Option<u32>> {
    filenames
        .iter()
        .map(|f| parse_track_number_with_preference(f, Some(preference)))
        .collect()
}

/// Prompts the user (via the provider) to enter a track number for each file.
/// An empty answer assigns no track number for that file.
async fn collect_manual_numbers(
    provider: &dyn InteractionProvider,
    filenames: &[String],
) -> Result<Vec<Option<u32>>, HvtError> {
    let mut numbers: Vec<Option<u32>> = Vec::with_capacity(filenames.len());
    for filename in filenames {
        numbers.push(provider.input_manual_track_number(filename).await?);
    }
    Ok(numbers)
}
