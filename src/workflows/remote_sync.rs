//! Pulls each configured `import.remote_sources` machine's drop folder into `import.source_path`
//! via `rsync` over `ssh`, before `--full` scans that directory. Best-effort per source: one
//! unreachable machine is logged and skipped rather than aborting the whole import — the same
//! "continue past individual failures" convention the rest of the import pipeline follows.

use crate::config::{Config, RemoteSource};
use crate::errors::HvtError;
use crate::interaction::progress::ProgressSink;

/// Pulls every configured remote source into `import.source_path`. A no-op (no phase reported)
/// when `import.remote_sources` is empty, so nothing changes for anyone not using this.
pub async fn collect_remote_sources(config: &Config, progress: &dyn ProgressSink) -> Result<(), HvtError> {
    collect_remote_sources_with_bin(config, progress, "rsync").await
}

async fn collect_remote_sources_with_bin(
    config: &Config,
    progress: &dyn ProgressSink,
    rsync_bin: &str,
) -> Result<(), HvtError> {
    let sources = &config.import.remote_sources;
    if sources.is_empty() {
        return Ok(());
    }

    let source_path = config.import.source_path.as_ref().ok_or_else(|| {
        HvtError::Generic("import.source_path must be configured to use import.remote_sources".to_string())
    })?;

    std::fs::create_dir_all(source_path)
        .map_err(|e| HvtError::Generic(format!("Failed to create source directory {}: {}", source_path, e)))?;

    if !is_rsync_available(rsync_bin) {
        return Err(HvtError::Generic(format!(
            "'{}' not found in PATH — required to pull import.remote_sources",
            rsync_bin
        )));
    }

    progress.phase("Collecting remote sources");
    progress.start_step(sources.len() as u64);

    for source in sources {
        let message = match pull_one(rsync_bin, source, source_path).await {
            Ok(()) => format!("{} ✓", source.name),
            Err(e) => {
                progress.log(&format!("Failed to pull from '{}': {}", source.name, e));
                format!("{} ✗", source.name)
            }
        };
        progress.item(&message);
    }

    progress.finish_step();
    Ok(())
}

fn is_rsync_available(rsync_bin: &str) -> bool {
    std::process::Command::new(rsync_bin)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Runs one `rsync -az -e "ssh ..." user@host:remote_path/ source_path/`. `remote_path`'s
/// *contents* are pulled, not the directory itself — matching how works sit directly under a
/// drop folder rather than nested in one more level.
async fn pull_one(rsync_bin: &str, source: &RemoteSource, source_path: &str) -> Result<(), HvtError> {
    let mut ssh_cmd = format!(
        "ssh -p {} -o BatchMode=yes -o StrictHostKeyChecking=accept-new",
        source.port
    );
    if let Some(key) = &source.ssh_key_path {
        ssh_cmd.push_str(&format!(" -i {}", key));
    }

    let remote_spec = format!(
        "{}@{}:{}/",
        source.user,
        source.host,
        source.remote_path.trim_end_matches('/')
    );

    let mut cmd = tokio::process::Command::new(rsync_bin);
    cmd.arg("-az").arg("-e").arg(&ssh_cmd).arg(&remote_spec).arg(source_path);
    if source.remove_after_pull {
        cmd.arg("--remove-source-files");
    }

    let output = cmd.output().await.map_err(|e| {
        HvtError::Generic(format!(
            "Failed to run '{}' for remote source '{}': {}",
            rsync_bin, source.name, e
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HvtError::Generic(format!(
            "rsync exited with {}: {}",
            output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "a signal".to_string()),
            stderr.trim()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingProgress {
        logs: Mutex<Vec<String>>,
        items: Mutex<Vec<String>>,
    }

    impl RecordingProgress {
        fn new() -> Self {
            RecordingProgress { logs: Mutex::new(Vec::new()), items: Mutex::new(Vec::new()) }
        }
    }

    impl ProgressSink for RecordingProgress {
        fn phase(&self, _name: &str) {}
        fn start_step(&self, _total: u64) {}
        fn item(&self, message: &str) {
            self.items.lock().unwrap().push(message.to_string());
        }
        fn finish_step(&self) {}
        fn log(&self, message: &str) {
            self.logs.lock().unwrap().push(message.to_string());
        }
    }

    fn write_fake_rsync(dir: &std::path::Path, name: &str, body: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{}\n", body)).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path.to_string_lossy().to_string()
    }

    fn sample_source(remove_after_pull: bool) -> RemoteSource {
        RemoteSource {
            name: "test-machine".to_string(),
            host: "testhost".to_string(),
            port: 2222,
            user: "testuser".to_string(),
            ssh_key_path: Some("/fake/key".to_string()),
            remote_path: "/remote/drop".to_string(),
            remove_after_pull,
        }
    }

    #[tokio::test]
    async fn builds_the_expected_rsync_invocation() {
        let tmp = tempfile_dir();
        let recorded = tmp.join("recorded_args");
        let fake_rsync = write_fake_rsync(
            &tmp,
            "fake_rsync",
            &format!("printf '%s\\n' \"$@\" > {}\nexit 0", recorded.display()),
        );

        let mut config = Config::default();
        config.import.source_path = Some(tmp.join("source").to_string_lossy().to_string());
        config.import.remote_sources = vec![sample_source(false)];

        let progress = RecordingProgress::new();
        collect_remote_sources_with_bin(&config, &progress, &fake_rsync).await.unwrap();

        assert_eq!(progress.items.lock().unwrap().as_slice(), ["test-machine ✓"]);

        let recorded_args = std::fs::read_to_string(&recorded).unwrap();
        assert!(recorded_args.contains("-az"));
        assert!(recorded_args.contains("-p 2222"));
        assert!(recorded_args.contains("-i /fake/key"));
        assert!(recorded_args.contains("StrictHostKeyChecking=accept-new"));
        assert!(recorded_args.contains("testuser@testhost:/remote/drop/"));
        assert!(!recorded_args.contains("--remove-source-files"));
    }

    #[tokio::test]
    async fn remove_after_pull_adds_the_flag() {
        let tmp = tempfile_dir();
        let recorded = tmp.join("recorded_args");
        let fake_rsync = write_fake_rsync(
            &tmp,
            "fake_rsync",
            &format!("printf '%s\\n' \"$@\" > {}\nexit 0", recorded.display()),
        );

        let mut config = Config::default();
        config.import.source_path = Some(tmp.join("source").to_string_lossy().to_string());
        config.import.remote_sources = vec![sample_source(true)];

        let progress = RecordingProgress::new();
        collect_remote_sources_with_bin(&config, &progress, &fake_rsync).await.unwrap();

        let recorded_args = std::fs::read_to_string(&recorded).unwrap();
        assert!(recorded_args.contains("--remove-source-files"));
    }

    #[tokio::test]
    async fn a_failed_pull_is_logged_but_does_not_abort_the_batch() {
        let tmp = tempfile_dir();
        let fake_rsync = write_fake_rsync(
            &tmp,
            "fake_rsync",
            "if [ \"$1\" = \"--version\" ]; then exit 0; fi\necho 'boom' >&2\nexit 3",
        );

        let mut config = Config::default();
        config.import.source_path = Some(tmp.join("source").to_string_lossy().to_string());
        config.import.remote_sources = vec![sample_source(false)];

        let progress = RecordingProgress::new();
        let result = collect_remote_sources_with_bin(&config, &progress, &fake_rsync).await;

        assert!(result.is_ok(), "a per-source failure should not fail the whole batch");
        assert_eq!(progress.items.lock().unwrap().as_slice(), ["test-machine ✗"]);
        let logs = progress.logs.lock().unwrap();
        assert!(logs.iter().any(|l| l.contains("Failed to pull from 'test-machine'") && l.contains("boom")));
    }

    #[tokio::test]
    async fn no_remote_sources_is_a_silent_no_op() {
        let config = Config::default();
        let progress = RecordingProgress::new();
        collect_remote_sources_with_bin(&config, &progress, "rsync-binary-that-does-not-exist")
            .await
            .unwrap();
        assert!(progress.items.lock().unwrap().is_empty());
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hvtag-remote-sync-test-{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn uuid_like() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}-{}", nanos, std::process::id(), count)
    }
}
