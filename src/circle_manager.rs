use dialoguer::{Select, Input, Confirm, theme::ColorfulTheme};
use rusqlite::Connection;
use crate::errors::HvtError;
use crate::database::custom_circles::{self, CirclePreferenceType};

pub fn run_interactive_circle_manager(conn: &Connection) -> Result<(), HvtError> {
    loop {
        // Main menu
        let options = vec![
            "View all circles (alphabetically)",
            "Set circle preference (global)",
            "View current circle preferences",
            "Remove circle preference",
            "Exit"
        ];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Circle Manager - Main Menu")
            .items(&options)
            .default(0)
            .interact()
            .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

        match selection {
            0 => view_all_circles(conn)?,
            1 => set_circle_preference(conn)?,
            2 => view_circle_preferences(conn)?,
            3 => remove_circle_preference(conn)?,
            4 => {
                println!("Exiting circle manager...");
                break;
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn view_all_circles(conn: &Connection) -> Result<(), HvtError> {
    let circles = custom_circles::list_all_circles(conn)?;

    if circles.is_empty() {
        println!("\nNo circles found in database.");
        println!("Run --collect first to fetch metadata from DLSite.");
        return Ok(());
    }

    println!("\n=== All Circles (Alphabetically) ===");
    for (_cir_id, rgcode, name_en, name_jp, pref_type, custom_name) in &circles {
        let display_name = if !name_jp.is_empty() {
            name_jp
        } else if !name_en.is_empty() {
            name_en
        } else {
            rgcode
        };

        if let Some(pref) = pref_type {
            match pref.as_str() {
                "force_en" => println!("  {} ({}) → force EN: {}", display_name, rgcode, name_en),
                "force_jp" => println!("  {} ({}) → force JP: {}", display_name, rgcode, name_jp),
                "custom" => {
                    if let Some(custom) = custom_name {
                        println!("  {} ({}) → custom: {}", display_name, rgcode, custom);
                    }
                }
                "use_code" => println!("  {} ({}) → use code: {}", display_name, rgcode, rgcode),
                _ => println!("  {} ({})", display_name, rgcode),
            }
        } else {
            // No preference, show default (JP → EN)
            if !name_jp.is_empty() {
                println!("  {} ({}) [JP name - default]", name_jp, rgcode);
            } else if !name_en.is_empty() {
                println!("  {} ({}) [EN name - fallback]", name_en, rgcode);
            } else {
                println!("  {} [code only]", rgcode);
            }
        }
    }
    println!("\nTotal: {} circles", circles.len());
    println!();

    Ok(())
}

fn set_circle_preference(conn: &Connection) -> Result<(), HvtError> {
    let circles = custom_circles::list_all_circles(conn)?;

    if circles.is_empty() {
        println!("\nNo circles found in database.");
        return Ok(());
    }

    // Create display strings (sorted alphabetically by JP → EN → code)
    let circle_displays: Vec<String> = circles.iter()
        .map(|(_id, rgcode, name_en, name_jp, pref_type, custom_name)| {
            let display_name = if !name_jp.is_empty() {
                name_jp.clone()
            } else if !name_en.is_empty() {
                name_en.clone()
            } else {
                rgcode.clone()
            };

            if let Some(pref) = pref_type {
                match pref.as_str() {
                    "force_en" => format!("{} ({}) [force EN]", display_name, rgcode),
                    "force_jp" => format!("{} ({}) [force JP]", display_name, rgcode),
                    "custom" => {
                        if let Some(custom) = custom_name {
                            format!("{} ({}) [custom: {}]", display_name, rgcode, custom)
                        } else {
                            format!("{} ({})", display_name, rgcode)
                        }
                    }
                    "use_code" => format!("{} ({}) [use code]", display_name, rgcode),
                    _ => format!("{} ({})", display_name, rgcode),
                }
            } else {
                format!("{} ({})", display_name, rgcode)
            }
        })
        .collect();

    // Select circle
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a circle to set preference (this will affect ALL works by this circle)")
        .items(&circle_displays)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let (_cir_id, rgcode, name_en, name_jp, _current_pref, _current_custom) = &circles[selection];

    // Show affected works
    let affected_works = custom_circles::get_works_using_circle(conn, rgcode)?;
    println!("\n=== Works by circle '{}' ===", rgcode);

    if affected_works.is_empty() {
        println!("No works currently registered for this circle.");
        return Ok(());
    }

    println!("This circle has {} work(s):", affected_works.len());
    for (i, (rjcode, name)) in affected_works.iter().enumerate() {
        if i < 5 {
            println!("  - {}: {}", rjcode, name);
        }
    }
    if affected_works.len() > 5 {
        println!("  ... and {} more", affected_works.len() - 5);
    }
    println!();

    // Show available names
    println!("Available names for this circle:");
    if !name_jp.is_empty() {
        println!("  JP: {}", name_jp);
    } else {
        println!("  JP: (empty)");
    }
    if !name_en.is_empty() {
        println!("  EN: {}", name_en);
    } else {
        println!("  EN: (empty)");
    }
    println!("  Code: {}", rgcode);
    println!();

    // Select preference type
    let pref_options = vec![
        format!("Force JP name ({})", if !name_jp.is_empty() { name_jp } else { "(empty)" }),
        format!("Force EN name ({})", if !name_en.is_empty() { name_en } else { "(empty)" }),
        "Custom name (enter manually)".to_string(),
        format!("Use RG code ({})", rgcode),
        "Cancel".to_string()
    ];

    let pref_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Choose preference for circle '{}' ({} works)", rgcode, affected_works.len()))
        .items(&pref_options)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let (preference_type, custom_name_opt) = match pref_selection {
        0 => (CirclePreferenceType::ForceJp, None),
        1 => (CirclePreferenceType::ForceEn, None),
        2 => {
            // Get custom name
            let default_name = if !name_jp.is_empty() {
                name_jp.clone()
            } else if !name_en.is_empty() {
                name_en.clone()
            } else {
                rgcode.clone()
            };

            let custom_name: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter custom circle name")
                .with_initial_text(&default_name)
                .interact_text()
                .map_err(|e| HvtError::Parse(format!("Input error: {}", e)))?;

            if custom_name.trim().is_empty() {
                println!("Custom name cannot be empty. Cancelled.");
                return Ok(());
            }

            (CirclePreferenceType::Custom, Some(custom_name.trim().to_string()))
        }
        3 => (CirclePreferenceType::UseCode, None),
        4 => {
            println!("Cancelled.");
            return Ok(());
        }
        _ => unreachable!(),
    };

    // Determine final display name
    let final_name = match preference_type {
        CirclePreferenceType::ForceJp => name_jp.clone(),
        CirclePreferenceType::ForceEn => name_en.clone(),
        CirclePreferenceType::Custom => custom_name_opt.clone().unwrap(),
        CirclePreferenceType::UseCode => rgcode.clone(),
    };

    // Confirm the preference
    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Set circle '{}' to '{}' for {} work(s)?",
            rgcode,
            final_name,
            affected_works.len()
        ))
        .default(true)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Set the preference
    custom_circles::set_circle_preference(
        conn,
        rgcode,
        preference_type,
        custom_name_opt.as_deref(),
    )?;
    println!("\n✓ Circle preference set successfully!");

    // Mark all affected works for re-tagging
    let files_marked = custom_circles::mark_circle_works_for_retagging(conn, rgcode)?;

    if files_marked > 0 {
        println!("✓ {} file(s) marked for re-tagging", files_marked);
        println!("  Run --tag to apply changes to all affected works");
    } else {
        println!("  No files were marked for re-tagging (they may not have been tagged yet)");
    }

    Ok(())
}

fn view_circle_preferences(conn: &Connection) -> Result<(), HvtError> {
    let prefs = custom_circles::get_all_custom_circle_preferences(conn)?;

    if prefs.is_empty() {
        println!("\nNo custom circle preferences found.");
        println!("Use 'Set circle preference' to create custom preferences.");
        return Ok(());
    }

    println!("\n=== Current Circle Preferences ===");
    for (rgcode, name_en, name_jp, pref_type, custom_name) in &prefs {
        let display_name = if !name_jp.is_empty() {
            name_jp
        } else if !name_en.is_empty() {
            name_en
        } else {
            rgcode
        };

        let affected_works = custom_circles::get_works_using_circle(conn, rgcode)?;

        match pref_type.as_str() {
            "force_en" => println!("  {} ({}) → force EN: {} ({} works)", display_name, rgcode, name_en, affected_works.len()),
            "force_jp" => println!("  {} ({}) → force JP: {} ({} works)", display_name, rgcode, name_jp, affected_works.len()),
            "custom" => {
                if let Some(custom) = custom_name {
                    println!("  {} ({}) → custom: {} ({} works)", display_name, rgcode, custom, affected_works.len());
                }
            }
            "use_code" => println!("  {} ({}) → use code: {} ({} works)", display_name, rgcode, rgcode, affected_works.len()),
            _ => {}
        }
    }
    println!("\nTotal: {} custom preferences", prefs.len());
    println!();

    Ok(())
}

fn remove_circle_preference(conn: &Connection) -> Result<(), HvtError> {
    let prefs = custom_circles::get_all_custom_circle_preferences(conn)?;

    if prefs.is_empty() {
        println!("\nNo custom circle preferences to remove.");
        return Ok(());
    }

    // Create display strings with work counts
    let mut pref_displays = Vec::new();
    for (rgcode, name_en, name_jp, pref_type, custom_name) in &prefs {
        let display_name = if !name_jp.is_empty() {
            name_jp
        } else if !name_en.is_empty() {
            name_en
        } else {
            rgcode
        };

        let affected_works = custom_circles::get_works_using_circle(conn, rgcode)?;

        let display = match pref_type.as_str() {
            "force_en" => format!("{} ({}) [force EN] - {} work(s)", display_name, rgcode, affected_works.len()),
            "force_jp" => format!("{} ({}) [force JP] - {} work(s)", display_name, rgcode, affected_works.len()),
            "custom" => {
                if let Some(custom) = custom_name {
                    format!("{} ({}) [custom: {}] - {} work(s)", display_name, rgcode, custom, affected_works.len())
                } else {
                    format!("{} ({}) - {} work(s)", display_name, rgcode, affected_works.len())
                }
            }
            "use_code" => format!("{} ({}) [use code] - {} work(s)", display_name, rgcode, affected_works.len()),
            _ => format!("{} ({}) - {} work(s)", display_name, rgcode, affected_works.len()),
        };

        pref_displays.push(display);
    }

    // Select preference to remove
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a circle preference to remove (will revert to default JP → EN → Unknown)")
        .items(&pref_displays)
        .default(0)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Selection error: {}", e)))?;

    let (rgcode, name_en, name_jp, pref_type, custom_name) = &prefs[selection];
    let affected_works = custom_circles::get_works_using_circle(conn, rgcode)?;

    let default_name = if !name_jp.is_empty() {
        name_jp
    } else if !name_en.is_empty() {
        name_en
    } else {
        "Unknown Circle"
    };

    // Confirm removal
    let empty_string = String::from("");
    let current_name = match pref_type.as_str() {
        "force_en" => name_en,
        "force_jp" => name_jp,
        "custom" => custom_name.as_ref().unwrap_or(&empty_string),
        "use_code" => rgcode,
        _ => "",
    };

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Remove preference for circle '{}'? (affects {} works, will revert from '{}' to '{}')",
            rgcode,
            affected_works.len(),
            current_name,
            default_name
        ))
        .default(true)
        .interact()
        .map_err(|e| HvtError::Parse(format!("Confirmation error: {}", e)))?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Remove the preference
    custom_circles::remove_circle_preference(conn, rgcode)?;
    println!("\n✓ Circle preference removed successfully!");

    // Mark all affected works for re-tagging
    let files_marked = custom_circles::mark_circle_works_for_retagging(conn, rgcode)?;

    if files_marked > 0 {
        println!("✓ {} file(s) marked for re-tagging", files_marked);
        println!("  Run --tag to apply changes to all affected works");
    } else {
        println!("  No files were marked for re-tagging");
    }

    Ok(())
}
