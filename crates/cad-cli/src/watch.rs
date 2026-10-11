//! Recipe-byte polling around the ordinary atomic build/publish path.
//!
//! Watch status is human-readable stderr only. Successful generations retain
//! their machine-readable manifest; stdout is not a stream of partial reports.
use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

type BuildAttempt = Result<Value, Box<dyn Error>>;

#[derive(PartialEq, Eq)]
enum SourceState {
    Bytes(Vec<u8>),
    Unreadable(String),
}

#[derive(Default)]
struct WatchState {
    previous: Option<SourceState>,
}

enum PollResult {
    Unchanged,
    Built(Value),
    Failed(String),
}

impl WatchState {
    fn poll(&mut self, path: &Path, build: &mut impl FnMut(&[u8]) -> BuildAttempt) -> PollResult {
        let current = match fs::read(path) {
            Ok(bytes) => SourceState::Bytes(bytes),
            Err(error) => SourceState::Unreadable(error.to_string()),
        };
        if self.previous.as_ref() == Some(&current) {
            return PollResult::Unchanged;
        }
        // Record unsuccessful edits too: retry on the next source change, not on
        // every timer tick. A disappearing/reappearing file is a new state.
        let read_error = match &current {
            SourceState::Unreadable(message) => Some(message.clone()),
            SourceState::Bytes(_) => None,
        };
        self.previous = Some(current);
        if let Some(message) = read_error {
            return PollResult::Failed(format!("cannot read recipe: {message}"));
        }
        let Some(SourceState::Bytes(bytes)) = self.previous.as_ref() else {
            unreachable!("unreadable state returned above");
        };
        match build(bytes) {
            Ok(summary) => PollResult::Built(summary),
            Err(error) => PollResult::Failed(error.to_string()),
        }
    }
}

fn concise(message: &str) -> String {
    let flattened = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() <= 1000 {
        flattened
    } else {
        format!("{}…", flattened.chars().take(1000).collect::<String>())
    }
}

/// Build initially, then rebuild when recipe bytes change. Build failures are
/// recoverable and are logged once per edit. The callback must use the same
/// complete-generation publication path as `cad build` so errors retain the last
/// successful artifacts. The callback receives the exact observed bytes and
/// must not reload the path, since edits can race with the geometry build.
/// The process runs until interrupted (normally Ctrl-C).
/// It watches the root recipe; imported dependencies are not currently supported.
pub(super) fn run(
    recipe: &Path,
    interval: Duration,
    mut build: impl FnMut(&[u8]) -> BuildAttempt,
) -> BuildAttempt {
    if interval.is_zero() {
        return Err("watch interval must be greater than zero".into());
    }
    eprintln!("watch: {} (Ctrl-C to stop)", recipe.display());
    let mut watcher = WatchState::default();
    loop {
        match watcher.poll(recipe, &mut build) {
            PollResult::Unchanged => {}
            PollResult::Built(summary) => {
                let outputs = summary["outputs"]
                    .as_object()
                    .map_or(0, |outputs| outputs.len());
                let computed = summary["stats"]["computed_solids"].as_u64().unwrap_or(0);
                let cached = summary["stats"]["cache_hits"].as_u64().unwrap_or(0);
                eprintln!("watch: built {outputs} output(s), {computed} computed, {cached} cached");
            }
            PollResult::Failed(message) => {
                eprintln!(
                    "watch: build failed; keeping previous artifacts: {}",
                    concise(&message)
                );
            }
        }
        std::thread::sleep(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn watches_bytes_not_timestamps_and_does_not_repeat_failed_edits() {
        let directory = tempfile::tempdir().unwrap();
        let recipe = directory.path().join("part.star");
        fs::write(&recipe, "initial").unwrap();
        let mut watcher = WatchState::default();
        let mut attempts = 0;
        let mut build = |bytes: &[u8]| {
            attempts += 1;
            if bytes == b"broken" {
                Err("recipe is broken".into())
            } else {
                Ok(json!({"outputs":{},"stats":{}}))
            }
        };
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Built(_)
        ));
        fs::write(&recipe, "initial").unwrap();
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Unchanged
        ));
        fs::write(&recipe, "broken").unwrap();
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Failed(_)
        ));
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Unchanged
        ));
        fs::write(&recipe, "repaired").unwrap();
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Built(_)
        ));
        assert_eq!(attempts, 3);
    }

    #[test]
    fn missing_or_replaced_files_recover_without_repeated_errors() {
        let directory = tempfile::tempdir().unwrap();
        let recipe = directory.path().join("part.star");
        let mut watcher = WatchState::default();
        let mut attempts = 0;
        let mut build = |_bytes: &[u8]| {
            attempts += 1;
            Ok(json!({}))
        };
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Failed(_)
        ));
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Unchanged
        ));
        fs::write(&recipe, "ready").unwrap();
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Built(_)
        ));
        fs::remove_file(&recipe).unwrap();
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Failed(_)
        ));
        fs::write(&recipe, "ready").unwrap();
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Built(_)
        ));
        assert_eq!(attempts, 2);
    }

    fn plate(width: f64) -> String {
        format!("p=rectangle(id='profile',width={width},depth=4)\ns=extrude(id='body',profile=p,height=2)\noutput(name='plate',feature=s)\n")
    }

    fn artifacts(path: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        fs::read_dir(path)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn real_build_failures_preserve_complete_generation_then_recover() {
        let directory = tempfile::tempdir().unwrap();
        let recipe = directory.path().join("part.star");
        let output = directory.path().join("out");
        let cache = directory.path().join("cache");
        fs::write(&recipe, plate(10.0)).unwrap();
        let mut watcher = WatchState::default();
        let mut build = |bytes: &[u8]| {
            crate::run_with_source(
                crate::Cli {
                    command: crate::Command::Build {
                        display_only: false,
                        outputs: Vec::new(),
                        recipe: recipe.clone(),
                        out_dir: output.clone(),
                    },
                    params: vec![],
                    cache_dir: Some(cache.clone()),
                },
                Some(bytes),
            )
        };
        let PollResult::Built(first) = watcher.poll(&recipe, &mut build) else {
            panic!("initial build failed");
        };
        assert_eq!(first["outputs"]["plate"]["volume_mm3"], 80.0);
        let saved = artifacts(&output);
        fs::write(&recipe, "p = rectangle(").unwrap();
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Failed(_)
        ));
        assert_eq!(artifacts(&output), saved);
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Unchanged
        ));
        // A geometric failure must also leave every old artifact byte intact.
        fs::write(&recipe, plate(-1.0)).unwrap();
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Failed(_)
        ));
        assert_eq!(artifacts(&output), saved);
        fs::write(&recipe, plate(12.0)).unwrap();
        let PollResult::Built(repaired) = watcher.poll(&recipe, &mut build) else {
            panic!("repaired build failed");
        };
        assert_eq!(repaired["outputs"]["plate"]["volume_mm3"], 96.0);
        assert_ne!(artifacts(&output), saved);
        // Returning to earlier geometry should reuse the persistent cache.
        fs::write(&recipe, plate(10.0)).unwrap();
        let PollResult::Built(reverted) = watcher.poll(&recipe, &mut build) else {
            panic!("reverted build failed");
        };
        assert_eq!(reverted["stats"]["disk_cache_hits"], 1);
    }

    #[test]
    fn zero_interval_is_rejected_before_building() {
        assert!(
            run(Path::new("unused.star"), Duration::ZERO, |_bytes| panic!(
                "must not build"
            ))
            .is_err()
        );
    }

    #[test]
    fn build_uses_captured_bytes_when_source_changes_and_reverts_during_build() {
        let directory = tempfile::tempdir().unwrap();
        let recipe = directory.path().join("part.star");
        let output = directory.path().join("out");
        fs::write(&recipe, plate(10.0)).unwrap();
        let mut watcher = WatchState::default();
        let mut build = |snapshot: &[u8]| {
            // Simulate an editor changing the source between polling and the
            // ordinary recipe-loading point, then reverting before the next poll.
            fs::write(&recipe, plate(20.0))?;
            let result = crate::run_with_source(
                crate::Cli {
                    command: crate::Command::Build {
                        display_only: false,
                        outputs: Vec::new(),
                        recipe: recipe.clone(),
                        out_dir: output.clone(),
                    },
                    params: vec![],
                    cache_dir: None,
                },
                Some(snapshot),
            );
            fs::write(&recipe, plate(10.0))?;
            result
        };
        let PollResult::Built(summary) = watcher.poll(&recipe, &mut build) else {
            panic!("snapshot build failed");
        };
        assert_eq!(summary["outputs"]["plate"]["volume_mm3"], 80.0);
        assert!(matches!(
            watcher.poll(&recipe, &mut build),
            PollResult::Unchanged
        ));
        let manifest: Value =
            serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["outputs"]["plate"]["volume_mm3"], 80.0);
    }
}
