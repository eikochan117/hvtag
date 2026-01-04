use dialoguer::{Select, Input, Confirm, theme::ColorfulTheme};
use rusqlite::Connection;
use crate::errors::HvtError;
use crate::database::custom_tags;

pub fn run_interactive_tag_manager(conn: &Connection) -> Result<(), HvtError> {
    loop {
        // Main menu
        let options = vec![
            "View all tags (alphabetically)",
            "Rename a DLSite tag (global)",
            "Ignore a DLSite tag (global)",
            "Un-ignore a tag",
            "Bulk ignore tags below threshold",
            "View current custom mappings",
            "Remove a custom mapping",
            "Exit"
        ];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Tag Manager - Main Menu")
            .items(&options)
            .default(0)
            .interact()
            .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

        match selection {
            0 => view_all_tags(conn)?,
            1 => rename_tag(conn)?,
            2 => ignore_tag(conn)?,
            3 => unignore_tag(conn)?,
            4 => bulk_ignore_tags_below_threshold(conn)?,
            5 => view_custom_mappings(conn)?,
            6 => remove_custom_mapping(conn)?,
            7 => {
                println!("Exiting tag manager...");
                break;
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn view_all_tags(conn: &Connection) -> Result<(), HvtError> {
    let tags = custom_tags::list_all_dlsite_tags_with_counts(conn)?;

    if tags.is_empty() {
        println!("\nNo tags found in database.");
        println!("Run --collect first to fetch metadata from DLSite.");
        return Ok(());
    }

    println!("\n=== All DLSite Tags (Alphabetically) ===");
    for (_tag_id, tag_name, custom_name, is_ignored, work_count) in &tags {
        if *is_ignored {
            println!("  {} ({}) (ignored)", tag_name, work_count);
        } else if let Some(custom) = custom_name {
            println!("  {} → {} ({}) (custom)", tag_name, custom, work_count);
        } else {
            println!("  {} ({})", tag_name, work_count);
        }
    }
    println!("\nTotal: {} tags", tags.len());
    println!();

    Ok(())
}

fn rename_tag(conn: &Connection) -> Result<(), HvtError> {
    let tags = custom_tags::list_all_dlsite_tags_with_counts(conn)?;

    if tags.is_empty() {
        println!("\nNo tags found in database.");
        return Ok(());
    }

    // Create display strings with work counts
    let tag_displays: Vec<String> = tags.iter()
        .map(|(_id, name, custom, is_ignored, work_count)| {
            if *is_ignored {
                format!("{} ({}) (ignored)", name, work_count)
            } else if let Some(custom_name) = custom {
                format!("{} → {} ({}) (custom)", name, custom_name, work_count)
            } else {
                format!("{} ({})", name, work_count)
            }
        })
        .collect();

    // Select tag to rename
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a DLSite tag to rename (this will affect ALL works)")
        .items(&tag_displays)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let (_tag_id, dlsite_tag_name, current_custom, _is_ignored, _work_count) = &tags[selection];

    // Show affected works
    let affected_works = custom_tags::get_works_using_tag(conn, dlsite_tag_name)?;
    println!("\n=== Works using tag '{}' ===", dlsite_tag_name);

    if affected_works.is_empty() {
        println!("No works currently use this tag.");
        return Ok(());
    }

    println!("This tag is used by {} work(s):", affected_works.len());
    for (i, (rjcode, name)) in affected_works.iter().enumerate() {
        if i < 5 {
            println!("  - {}: {}", rjcode, name);
        }
    }
    if affected_works.len() > 5 {
        println!("  ... and {} more", affected_works.len() - 5);
    }
    println!();

    // Get custom tag name
    let default_value = current_custom.clone().unwrap_or_else(|| dlsite_tag_name.clone());
    let custom_tag_name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Enter new name for '{}' (affects {} works)", dlsite_tag_name, affected_works.len()))
        .with_initial_text(&default_value)
        .interact_text()
        .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))?;

    if custom_tag_name.trim().is_empty() {
        println!("Tag name cannot be empty.");
        return Ok(());
    }

    // Confirm the rename
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Rename '{}' to '{}' for {} work(s)?",
            dlsite_tag_name,
            custom_tag_name.trim(),
            affected_works.len()
        ))
        .default(true)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Add the mapping
    custom_tags::add_custom_tag_mapping(conn, dlsite_tag_name, custom_tag_name.trim())?;
    println!("\n✓ Tag mapping added successfully!");

    // Mark all affected works for re-tagging
    let files_marked = custom_tags::mark_works_for_retagging(conn, dlsite_tag_name)?;

    if files_marked > 0 {
        println!("✓ {} file(s) marked for re-tagging", files_marked);
        println!("  Run --tag to apply changes to all affected works");
    } else {
        println!("  No files were marked for re-tagging (they may not have been tagged yet)");
    }

    Ok(())
}

fn ignore_tag(conn: &Connection) -> Result<(), HvtError> {
    let tags = custom_tags::list_all_dlsite_tags_with_counts(conn)?;

    if tags.is_empty() {
        println!("\nNo tags found in database.");
        return Ok(());
    }

    // Create display strings with work counts
    let tag_displays: Vec<String> = tags.iter()
        .map(|(_id, name, custom, is_ignored, work_count)| {
            if *is_ignored {
                format!("{} ({}) (already ignored)", name, work_count)
            } else if let Some(custom_name) = custom {
                format!("{} → {} ({}) (custom)", name, custom_name, work_count)
            } else {
                format!("{} ({})", name, work_count)
            }
        })
        .collect();

    // Select tag to ignore
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a DLSite tag to ignore (this will affect ALL works)")
        .items(&tag_displays)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let (_tag_id, dlsite_tag_name, _current_custom, _is_ignored, _work_count) = &tags[selection];

    // Show affected works
    let affected_works = custom_tags::get_works_using_tag(conn, dlsite_tag_name)?;
    println!("\n=== Works using tag '{}' ===", dlsite_tag_name);

    if affected_works.is_empty() {
        println!("No works currently use this tag.");
        return Ok(());
    }

    println!("This tag is used by {} work(s):", affected_works.len());
    for (i, (rjcode, name)) in affected_works.iter().enumerate() {
        if i < 5 {
            println!("  - {}: {}", rjcode, name);
        }
    }
    if affected_works.len() > 5 {
        println!("  ... and {} more", affected_works.len() - 5);
    }
    println!();

    // Confirm the ignore
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Ignore tag '{}' for {} work(s)? (will not appear in audio file tags)",
            dlsite_tag_name,
            affected_works.len()
        ))
        .default(true)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Mark tag as ignored
    custom_tags::ignore_tag(conn, dlsite_tag_name)?;
    println!("\n✓ Tag marked as ignored successfully!");

    // Mark all affected works for re-tagging
    let files_marked = custom_tags::mark_works_for_retagging(conn, dlsite_tag_name)?;

    if files_marked > 0 {
        println!("✓ {} file(s) marked for re-tagging", files_marked);
        println!("  Run --tag to apply changes to all affected works");
    } else {
        println!("  No files were marked for re-tagging (they may not have been tagged yet)");
    }

    Ok(())
}

fn unignore_tag(conn: &Connection) -> Result<(), HvtError> {
    let tags = custom_tags::list_all_dlsite_tags_with_counts(conn)?;

    // Filter to only ignored tags
    let ignored_tags: Vec<_> = tags.iter()
        .filter(|(_, _, _, is_ignored, _)| *is_ignored)
        .collect();

    if ignored_tags.is_empty() {
        println!("\nNo ignored tags found.");
        return Ok(());
    }

    // Create display strings with work counts
    let tag_displays: Vec<String> = ignored_tags.iter()
        .map(|(_, name, _, _, work_count)| {
            format!("{} ({} works)", name, work_count)
        })
        .collect();

    // Select tag to un-ignore
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a tag to un-ignore")
        .items(&tag_displays)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let (_, dlsite_tag_name, _, _, work_count) = ignored_tags[selection];

    // Confirm
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Un-ignore tag '{}'? (will appear again in {} work(s))",
            dlsite_tag_name,
            work_count
        ))
        .default(true)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Remove the ignore mapping
    custom_tags::remove_custom_tag_mapping(conn, dlsite_tag_name)?;
    println!("\n✓ Tag '{}' is no longer ignored!", dlsite_tag_name);

    // Mark all affected works for re-tagging
    let files_marked = custom_tags::mark_works_for_retagging(conn, dlsite_tag_name)?;

    if files_marked > 0 {
        println!("✓ {} file(s) marked for re-tagging", files_marked);
        println!("  Run --tag to apply changes to all affected works");
    } else {
        println!("  No files were marked for re-tagging");
    }

    Ok(())
}

fn bulk_ignore_tags_below_threshold(conn: &Connection) -> Result<(), HvtError> {
    let tags = custom_tags::list_all_dlsite_tags_with_counts(conn)?;

    if tags.is_empty() {
        println!("\nNo tags found in database.");
        return Ok(());
    }

    // Filter out already ignored tags
    let active_tags: Vec<_> = tags.iter()
        .filter(|(_, _, _, is_ignored, _)| !*is_ignored)
        .collect();

    if active_tags.is_empty() {
        println!("\nAll tags are already ignored.");
        return Ok(());
    }

    // Show current tag distribution
    let max_count = active_tags.iter().map(|(_, _, _, _, c)| *c).max().unwrap_or(0);
    let min_count = active_tags.iter().map(|(_, _, _, _, c)| *c).min().unwrap_or(0);
    println!("\n=== Tag Usage Statistics ===");
    println!("Total active tags: {}", active_tags.len());
    println!("Work count range: {} - {}", min_count, max_count);

    // Show distribution hints
    let below_5 = active_tags.iter().filter(|(_, _, _, _, c)| *c < 5).count();
    let below_10 = active_tags.iter().filter(|(_, _, _, _, c)| *c < 10).count();
    let below_20 = active_tags.iter().filter(|(_, _, _, _, c)| *c < 20).count();
    println!("\nTags with less than 5 works: {}", below_5);
    println!("Tags with less than 10 works: {}", below_10);
    println!("Tags with less than 20 works: {}", below_20);

    // Ask for threshold
    let threshold_options = vec![
        "Less than 5 works",
        "Less than 10 works",
        "Less than 20 works",
        "Custom threshold",
        "Cancel"
    ];

    let threshold_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("\nSelect threshold (tags below this will be ignored)")
        .items(&threshold_options)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let threshold: i64 = match threshold_selection {
        0 => 5,
        1 => 10,
        2 => 20,
        3 => {
            let input: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter custom threshold (ignore tags used by fewer works)")
                .validate_with(|input: &String| -> Result<(), &str> {
                    input.parse::<i64>().map(|_| ()).map_err(|_| "Please enter a valid number")
                })
                .interact_text()
                .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))?;
            input.parse().unwrap_or(5)
        }
        4 => {
            println!("Cancelled.");
            return Ok(());
        }
        _ => unreachable!(),
    };

    // Find tags to ignore
    let tags_to_ignore: Vec<_> = active_tags.iter()
        .filter(|(_, _, _, _, work_count)| *work_count < threshold)
        .collect();

    if tags_to_ignore.is_empty() {
        println!("\nNo tags found with fewer than {} works.", threshold);
        return Ok(());
    }

    // Show tags that will be ignored
    println!("\n=== Tags to be ignored ({} tags) ===", tags_to_ignore.len());
    for (i, (_, tag_name, custom_name, _, work_count)) in tags_to_ignore.iter().enumerate() {
        if i < 20 {
            if let Some(custom) = custom_name {
                println!("  {} → {} ({} works)", tag_name, custom, work_count);
            } else {
                println!("  {} ({} works)", tag_name, work_count);
            }
        }
    }
    if tags_to_ignore.len() > 20 {
        println!("  ... and {} more", tags_to_ignore.len() - 20);
    }

    // Confirm
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "\nIgnore {} tag(s) with fewer than {} works?",
            tags_to_ignore.len(),
            threshold
        ))
        .default(false)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Ignore all selected tags
    let mut ignored_count = 0;
    let mut files_marked_total = 0;

    for (_, tag_name, _, _, _) in &tags_to_ignore {
        if let Err(e) = custom_tags::ignore_tag(conn, tag_name) {
            println!("  Failed to ignore '{}': {}", tag_name, e);
            continue;
        }
        ignored_count += 1;

        // Mark works for re-tagging
        if let Ok(files_marked) = custom_tags::mark_works_for_retagging(conn, tag_name) {
            files_marked_total += files_marked;
        }
    }

    println!("\n✓ {} tag(s) marked as ignored", ignored_count);
    if files_marked_total > 0 {
        println!("✓ {} file(s) marked for re-tagging", files_marked_total);
        println!("  Run --tag to apply changes to all affected works");
    }

    Ok(())
}

fn view_custom_mappings(conn: &Connection) -> Result<(), HvtError> {
    let mappings = custom_tags::get_all_custom_mappings(conn)?;

    if mappings.is_empty() {
        println!("\nNo custom tag mappings found.");
        println!("Use 'Rename a DLSite tag' or 'Ignore a DLSite tag' to create custom mappings.");
        return Ok(());
    }

    println!("\n=== Current Custom Tag Mappings ===");
    for (dlsite_tag, custom_tag, is_ignored) in &mappings {
        let affected_works = custom_tags::get_works_using_tag(conn, dlsite_tag)?;
        if *is_ignored {
            println!("  {} (ignored) - {} work(s)", dlsite_tag, affected_works.len());
        } else if let Some(custom_name) = custom_tag {
            println!("  {} → {} ({} work(s))", dlsite_tag, custom_name, affected_works.len());
        }
    }
    println!("\nTotal: {} custom mappings", mappings.len());
    println!();

    Ok(())
}

fn remove_custom_mapping(conn: &Connection) -> Result<(), HvtError> {
    let mappings = custom_tags::get_all_custom_mappings(conn)?;

    if mappings.is_empty() {
        println!("\nNo custom tag mappings to remove.");
        return Ok(());
    }

    // Create display strings with work counts
    let mut mapping_displays = Vec::new();
    for (dlsite_tag, custom_tag, is_ignored) in &mappings {
        let affected_works = custom_tags::get_works_using_tag(conn, dlsite_tag)?;
        if *is_ignored {
            mapping_displays.push(format!(
                "{} (ignored) - {} work(s)",
                dlsite_tag,
                affected_works.len()
            ));
        } else if let Some(custom_name) = custom_tag {
            mapping_displays.push(format!(
                "{} → {} ({} work(s))",
                dlsite_tag,
                custom_name,
                affected_works.len()
            ));
        }
    }

    // Select mapping to remove
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a custom mapping to remove (will revert to DLSite name)")
        .items(&mapping_displays)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let (dlsite_tag_name, custom_tag_name, is_ignored) = &mappings[selection];
    let affected_works = custom_tags::get_works_using_tag(conn, dlsite_tag_name)?;

    // Confirm removal
    let confirm_message = if *is_ignored {
        format!(
            "Remove ignore flag for '{}'? (affects {} works, will show tag again)",
            dlsite_tag_name,
            affected_works.len()
        )
    } else {
        format!(
            "Remove custom mapping '{}' → '{}'? (affects {} works, will revert to '{}')",
            dlsite_tag_name,
            custom_tag_name.as_deref().unwrap_or(""),
            affected_works.len(),
            dlsite_tag_name
        )
    };

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(confirm_message)
        .default(true)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Remove the mapping
    custom_tags::remove_custom_tag_mapping(conn, dlsite_tag_name)?;
    println!("\n✓ Custom mapping removed successfully!");

    // Mark all affected works for re-tagging
    let files_marked = custom_tags::mark_works_for_retagging(conn, dlsite_tag_name)?;

    if files_marked > 0 {
        println!("✓ {} file(s) marked for re-tagging", files_marked);
        println!("  Run --tag to apply changes to all affected works");
    } else {
        println!("  No files were marked for re-tagging");
    }

    Ok(())
}
