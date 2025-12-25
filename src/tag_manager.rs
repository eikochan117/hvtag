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
            3 => view_custom_mappings(conn)?,
            4 => remove_custom_mapping(conn)?,
            5 => {
                println!("Exiting tag manager...");
                break;
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn view_all_tags(conn: &Connection) -> Result<(), HvtError> {
    let tags = custom_tags::list_all_dlsite_tags(conn)?;

    if tags.is_empty() {
        println!("\nNo tags found in database.");
        println!("Run --collect first to fetch metadata from DLSite.");
        return Ok(());
    }

    println!("\n=== All DLSite Tags (Alphabetically) ===");
    for (_tag_id, tag_name, custom_name, is_ignored) in &tags {
        if *is_ignored {
            println!("  {} (ignored)", tag_name);
        } else if let Some(custom) = custom_name {
            println!("  {} → {} (custom)", tag_name, custom);
        } else {
            println!("  {}", tag_name);
        }
    }
    println!("\nTotal: {} tags", tags.len());
    println!();

    Ok(())
}

fn rename_tag(conn: &Connection) -> Result<(), HvtError> {
    let tags = custom_tags::list_all_dlsite_tags(conn)?;

    if tags.is_empty() {
        println!("\nNo tags found in database.");
        return Ok(());
    }

    // Create display strings
    let tag_displays: Vec<String> = tags.iter()
        .map(|(_id, name, custom, is_ignored)| {
            if *is_ignored {
                format!("{} (ignored)", name)
            } else if let Some(custom_name) = custom {
                format!("{} → {} (custom)", name, custom_name)
            } else {
                name.clone()
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

    let (_tag_id, dlsite_tag_name, current_custom, _is_ignored) = &tags[selection];

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
    let tags = custom_tags::list_all_dlsite_tags(conn)?;

    if tags.is_empty() {
        println!("\nNo tags found in database.");
        return Ok(());
    }

    // Create display strings
    let tag_displays: Vec<String> = tags.iter()
        .map(|(_id, name, custom, is_ignored)| {
            if *is_ignored {
                format!("{} (already ignored)", name)
            } else if let Some(custom_name) = custom {
                format!("{} → {} (custom)", name, custom_name)
            } else {
                name.clone()
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

    let (_tag_id, dlsite_tag_name, _current_custom, _is_ignored) = &tags[selection];

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
