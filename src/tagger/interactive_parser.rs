use dialoguer::{Select, Input, theme::ColorfulTheme};
use regex::Regex;
use crate::errors::HvtError;
use crate::tagger::track_parser::{TrackParsingPreference, parse_track_number_with_preference};

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

/// Runs the full interactive track-parsing session.
///
/// Shows the file list, presents the strategy menu, previews the result,
/// and loops back to the menu if the user rejects the preview.
/// Returns only when the user accepts a result or explicitly skips.
pub fn run_interactive_parsing(
    filenames: &[String],
    rjcode: &str,
) -> Result<ParsingResult, HvtError> {
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

    loop {
        match pick_strategy()? {
            StrategyChoice::Skip => return Ok(ParsingResult::Skip),

            StrategyChoice::Manual => {
                let numbers = collect_manual_numbers(filenames)?;
                return Ok(ParsingResult::Manual(numbers));
            }

            StrategyChoice::Preference(pref) => {
                let results = test_strategy(filenames, &pref);
                match confirm_strategy(filenames, &results)? {
                    true  => return Ok(ParsingResult::Strategy(pref)),
                    false => println!("\nStrategy rejected — please pick another one.\n"),
                }
                // loop continues
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

enum StrategyChoice {
    Preference(TrackParsingPreference),
    Manual,
    Skip,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Shows the strategy selection menu and returns the user's choice.
fn pick_strategy() -> Result<StrategyChoice, HvtError> {
    let options = vec![
        "Asian full-width numbers  (０１２ → 012)",
        "Asian brackets            【01】 ［01］ 〔01〕 （01）",
        "Kanji episode markers     第01話  第01章  第01回",
        "Custom delimiter          (number followed by a pattern)",
        "Strip prefix then first number  (regex, e.g. s.*?_ strips s19_ from s19_01_track)",
        "First number in filename  (fallback)",
        "Manual numbering          (enter each track number by hand)",
        "Skip this folder          (no track numbers)",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Parsing strategy")
        .items(&options)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    match selection {
        0 => Ok(StrategyChoice::Preference(TrackParsingPreference {
            strategy_name: "asian_fullwidth".to_string(),
            custom_delimiter: None,
            use_asian_conversion: true,
            asian_format_type: Some("fullwidth".to_string()),
            strip_prefix_pattern: None,
        })),
        1 => Ok(StrategyChoice::Preference(TrackParsingPreference {
            strategy_name: "asian_brackets".to_string(),
            custom_delimiter: None,
            use_asian_conversion: true,
            asian_format_type: Some("asian_brackets".to_string()),
            strip_prefix_pattern: None,
        })),
        2 => Ok(StrategyChoice::Preference(TrackParsingPreference {
            strategy_name: "asian_kanji_episode".to_string(),
            custom_delimiter: None,
            use_asian_conversion: true,
            asian_format_type: Some("kanji_episode".to_string()),
            strip_prefix_pattern: None,
        })),
        3 => {
            let delimiter: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Delimiter before track numbers (e.g. \"_\", \"No.\")")
                .interact_text()
                .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))?;
            Ok(StrategyChoice::Preference(TrackParsingPreference {
                strategy_name: "custom_delimiter".to_string(),
                custom_delimiter: Some(delimiter),
                use_asian_conversion: false,
                asian_format_type: None,
                strip_prefix_pattern: None,
            }))
        }
        4 => {
            println!("\nRegex pattern to remove from the start of the filename before");
            println!("looking for the first number.");
            println!("Examples:");
            println!("  s.*?_     strips 's19_' from 's19_01_track'");
            println!("  ^\\[.*?\\]\\s*  strips '[se01] ' from '[se01] track name'");
            println!("  (?i)vol\\d+_  strips 'vol3_' (case-insensitive)");
            println!();
            let pattern: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Regex pattern to strip")
                .interact_text()
                .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))?;

            // Validate the regex before accepting it
            match Regex::new(&pattern) {
                Ok(_) => Ok(StrategyChoice::Preference(TrackParsingPreference {
                    strategy_name: "strip_prefix".to_string(),
                    custom_delimiter: None,
                    use_asian_conversion: false,
                    asian_format_type: None,
                    strip_prefix_pattern: Some(pattern),
                })),
                Err(e) => {
                    println!("Invalid regex: {}. Falling back to first-number strategy.", e);
                    Ok(StrategyChoice::Preference(TrackParsingPreference {
                        strategy_name: "first_number".to_string(),
                        custom_delimiter: None,
                        use_asian_conversion: false,
                        asian_format_type: None,
                        strip_prefix_pattern: None,
                    }))
                }
            }
        }
        5 => Ok(StrategyChoice::Preference(TrackParsingPreference {
            strategy_name: "first_number".to_string(),
            custom_delimiter: None,
            use_asian_conversion: false,
            asian_format_type: None,
            strip_prefix_pattern: None,
        })),
        6 => Ok(StrategyChoice::Manual),
        7 => Ok(StrategyChoice::Skip),
        _ => unreachable!(),
    }
}

/// Prompts the user to enter a track number for each file.
/// Pressing Enter without a value assigns no track number for that file.
fn collect_manual_numbers(filenames: &[String]) -> Result<Vec<Option<u32>>, HvtError> {
    println!("\n=== Manual Track Numbering ===");
    println!("Enter the track number for each file (leave blank for none):\n");

    let mut numbers: Vec<Option<u32>> = Vec::with_capacity(filenames.len());

    for filename in filenames {
        let input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(filename)
            .allow_empty(true)
            .interact_text()
            .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))?;

        let n = input.trim().parse::<u32>().ok().filter(|&v| v > 0 && v < 1000);
        if !input.trim().is_empty() && n.is_none() {
            println!("  (invalid number, skipping)");
        }
        numbers.push(n);
    }

    let assigned = numbers.iter().filter(|n| n.is_some()).count();
    println!("\nAssigned track numbers: {}/{}", assigned, filenames.len());

    Ok(numbers)
}

/// Applies a strategy to all filenames and returns the parsed track numbers.
fn test_strategy(filenames: &[String], preference: &TrackParsingPreference) -> Vec<Option<u32>> {
    filenames
        .iter()
        .map(|f| parse_track_number_with_preference(f, Some(preference)))
        .collect()
}

/// Shows a preview of parsed track numbers and asks the user to confirm.
/// Returns `true` if accepted, `false` if rejected (triggers menu re-display).
fn confirm_strategy(filenames: &[String], track_numbers: &[Option<u32>]) -> Result<bool, HvtError> {
    println!("\n=== Parsing Preview ===");

    let mut success = 0usize;
    let mut failed  = 0usize;

    for (filename, track) in filenames.iter().zip(track_numbers.iter()).take(10) {
        match track {
            Some(n) => { println!("  [{:>3}] {}", n, filename); success += 1; }
            None    => { println!("  [ ??] {}", filename);       failed  += 1; }
        }
    }

    if filenames.len() > 10 {
        println!("  ... and {} more", filenames.len() - 10);
        for t in track_numbers.iter().skip(10) {
            if t.is_some() { success += 1; } else { failed += 1; }
        }
    }

    println!("\nParsed: {}/{}", success, filenames.len());
    if failed > 0 {
        println!(
            "Warning: {}/{} file(s) could not be parsed — they will be tagged without a track number.",
            failed, filenames.len()
        );
    }

    dialoguer::Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Use this strategy?")
        .default(true)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))
}
