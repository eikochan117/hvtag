use dialoguer::{Select, Input, theme::ColorfulTheme};
use crate::errors::HvtError;
use crate::tagger::track_parser::{TrackParsingPreference, parse_track_number_with_preference};

/// Prompt user for track parsing strategy when automatic parsing fails
pub fn prompt_for_parsing_strategy(
    filenames: &[String],
    rjcode: &str,
) -> Result<TrackParsingPreference, HvtError> {
    println!("\n=== Track Number Parsing Failed ===");
    println!("Work: {}", rjcode);
    println!("\nFiles in this folder:");

    // Show first 10 files to help user identify pattern
    for (i, filename) in filenames.iter().take(10).enumerate() {
        println!("  {}. {}", i + 1, filename);
    }

    if filenames.len() > 10 {
        println!("  ... and {} more files", filenames.len() - 10);
    }

    println!("\nAutomatic track number detection failed for these files.");
    println!("Please select a parsing strategy:\n");

    let options = vec![
        "Asian full-width numbers (０１２ → 012)",
        "Asian brackets 【01】［01］〔01〕（01）",
        "Kanji episode markers (第01話、第01章)",
        "Custom delimiter (I'll specify)",
        "First number found (no delimiter)",
        "Skip this folder (don't tag)",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select parsing strategy")
        .items(&options)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    match selection {
        0 => {
            // Asian full-width
            Ok(TrackParsingPreference {
                strategy_name: "asian_fullwidth".to_string(),
                custom_delimiter: None,
                use_asian_conversion: true,
                asian_format_type: Some("fullwidth".to_string()),
            })
        }
        1 => {
            // Asian brackets
            Ok(TrackParsingPreference {
                strategy_name: "asian_brackets".to_string(),
                custom_delimiter: None,
                use_asian_conversion: true,
                asian_format_type: Some("asian_brackets".to_string()),
            })
        }
        2 => {
            // Kanji episodes
            Ok(TrackParsingPreference {
                strategy_name: "asian_kanji_episode".to_string(),
                custom_delimiter: None,
                use_asian_conversion: true,
                asian_format_type: Some("kanji_episode".to_string()),
            })
        }
        3 => {
            // Custom delimiter
            let delimiter: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter the delimiter character(s) before track numbers")
                .interact_text()
                .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))?;

            Ok(TrackParsingPreference {
                strategy_name: "custom_delimiter".to_string(),
                custom_delimiter: Some(delimiter),
                use_asian_conversion: false,
                asian_format_type: None,
            })
        }
        4 => {
            // First number
            Ok(TrackParsingPreference {
                strategy_name: "first_number".to_string(),
                custom_delimiter: None,
                use_asian_conversion: false,
                asian_format_type: None,
            })
        }
        5 => {
            // Skip
            Err(HvtError::Parse("User skipped folder".to_string()))
        }
        _ => unreachable!(),
    }
}

/// Test if a strategy works for the given filenames
pub fn test_strategy(
    filenames: &[String],
    preference: &TrackParsingPreference,
) -> Vec<Option<u32>> {
    filenames.iter()
        .map(|f| parse_track_number_with_preference(f, Some(preference)))
        .collect()
}

/// Show preview of parsed track numbers and ask for confirmation
pub fn confirm_strategy(
    filenames: &[String],
    track_numbers: &[Option<u32>],
) -> Result<bool, HvtError> {
    println!("\n=== Parsing Preview ===");

    let mut success_count = 0;
    let mut failure_count = 0;

    for (filename, track) in filenames.iter().zip(track_numbers.iter()).take(10) {
        match track {
            Some(num) => {
                println!("  [{}] {}", num, filename);
                success_count += 1;
            }
            None => {
                println!("  [??] {}", filename);
                failure_count += 1;
            }
        }
    }

    if filenames.len() > 10 {
        println!("  ... and {} more files", filenames.len() - 10);

        // Count all successes/failures
        for track in track_numbers.iter().skip(10) {
            if track.is_some() {
                success_count += 1;
            } else {
                failure_count += 1;
            }
        }
    }

    println!("\nSuccess: {}/{}", success_count, filenames.len());
    println!("Failed: {}/{}", failure_count, filenames.len());

    if failure_count > 0 {
        println!("\nWarning: Some files could not be parsed with this strategy.");
        println!("They will be tagged without track numbers.");
    }

    let confirm = dialoguer::Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Use this strategy?")
        .default(true)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))?;

    Ok(confirm)
}
