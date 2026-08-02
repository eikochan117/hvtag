//! Portable path storage: the database stores work folder paths as `$library/RJ123456`
//! (or `$source/...`) rather than a literal absolute path, so the same database works across
//! deployments where `import.library_path`/`import.source_path` point somewhere different —
//! e.g. moving from a bare-metal install to a Docker volume mount at `/library`.
//!
//! `to_stored_path` converts a real filesystem path to the placeholder form before writing it
//! to the database; `resolve_stored_path` expands it back using the *currently loaded* config
//! before it's used for any actual filesystem I/O.

use crate::config::Config;

const LIBRARY_VAR: &str = "$library";
const SOURCE_VAR: &str = "$source";

/// Converts an absolute path into its portable form if it lives under the configured library
/// or source directory (library checked first, since post-import paths are the common case).
/// Anything else — a path outside both directories, or one of those directories not being
/// configured at all — is returned unchanged, which is also what keeps this backward compatible
/// with rows written before this existed.
pub fn to_stored_path(config: &Config, path: &str) -> String {
    if let Some(library_path) = config.import.library_path.as_deref() {
        if let Some(rest) = strip_dir_prefix(path, library_path) {
            return format!("{LIBRARY_VAR}/{rest}");
        }
    }
    if let Some(source_path) = config.import.source_path.as_deref() {
        if let Some(rest) = strip_dir_prefix(path, source_path) {
            return format!("{SOURCE_VAR}/{rest}");
        }
    }
    path.to_string()
}

/// Expands a `$library`/`$source`-prefixed stored path back into a real filesystem path.
/// A stored value with no recognized placeholder — including every path written before this
/// existed — is returned unchanged, so un-migrated rows keep resolving to exactly what they
/// already said.
pub fn resolve_stored_path(config: &Config, stored: &str) -> String {
    if let Some(expanded) = expand_var(stored, LIBRARY_VAR, config.import.library_path.as_deref()) {
        return expanded;
    }
    if let Some(expanded) = expand_var(stored, SOURCE_VAR, config.import.source_path.as_deref()) {
        return expanded;
    }
    stored.to_string()
}

fn expand_var(stored: &str, var: &str, dir: Option<&str>) -> Option<String> {
    let dir = dir?;
    if stored == var {
        return Some(dir.to_string());
    }
    stored
        .strip_prefix(var)
        .and_then(|rest| rest.strip_prefix(['/', '\\']))
        .map(|rest| join(dir, rest))
}

fn strip_dir_prefix<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    let dir = dir.trim_end_matches(['/', '\\']);
    path.strip_prefix(dir)
        .and_then(|rest| rest.strip_prefix(['/', '\\']))
}

fn join(dir: &str, rest: &str) -> String {
    let dir = dir.trim_end_matches(['/', '\\']);
    format!("{dir}/{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(library: Option<&str>, source: Option<&str>) -> Config {
        let mut config = Config::default();
        config.import.library_path = library.map(|s| s.to_string());
        config.import.source_path = source.map(|s| s.to_string());
        config
    }

    #[test]
    fn round_trips_a_library_path() {
        let config = config_with(Some("/library"), Some("/import"));
        let stored = to_stored_path(&config, "/library/RJ123456");
        assert_eq!(stored, "$library/RJ123456");
        assert_eq!(resolve_stored_path(&config, &stored), "/library/RJ123456");
    }

    #[test]
    fn round_trips_a_source_path() {
        let config = config_with(Some("/library"), Some("/import"));
        let stored = to_stored_path(&config, "/import/RJ999999");
        assert_eq!(stored, "$source/RJ999999");
        assert_eq!(resolve_stored_path(&config, &stored), "/import/RJ999999");
    }

    #[test]
    fn resolves_across_a_deployment_change() {
        // Stored on one deployment (bare metal)...
        let old_config = config_with(Some("/home/eiko/Library"), Some("/home/eiko/Import"));
        let stored = to_stored_path(&old_config, "/home/eiko/Library/RJ123456");
        assert_eq!(stored, "$library/RJ123456");

        // ...resolved on another (Docker) without touching the database at all.
        let new_config = config_with(Some("/library"), Some("/import"));
        assert_eq!(resolve_stored_path(&new_config, &stored), "/library/RJ123456");
    }

    #[test]
    fn leaves_paths_outside_both_directories_unchanged() {
        let config = config_with(Some("/library"), Some("/import"));
        let stored = to_stored_path(&config, "/some/other/place/RJ123456");
        assert_eq!(stored, "/some/other/place/RJ123456");
        assert_eq!(resolve_stored_path(&config, &stored), "/some/other/place/RJ123456");
    }

    #[test]
    fn leaves_legacy_absolute_paths_unchanged_when_read_back() {
        // A row written before this feature existed — must keep resolving to itself, not be
        // misread as some other placeholder form.
        let config = config_with(Some("/library"), Some("/import"));
        assert_eq!(resolve_stored_path(&config, "/library/RJ777777"), "/library/RJ777777");
    }

    #[test]
    fn trash_subdirectory_round_trips_too() {
        let config = config_with(Some("/library"), Some("/import"));
        let stored = to_stored_path(&config, "/library/.trash/RJ123456");
        assert_eq!(stored, "$library/.trash/RJ123456");
        assert_eq!(resolve_stored_path(&config, &stored), "/library/.trash/RJ123456");
    }
}
