//! The durable set of listing URLs that have already been notified.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::Path;

use anyhow::{Context, Result};

const FILE: &str = "seen-urls.txt";
const TEMP: &str = "seen-urls.txt.tmp";

pub fn load(state_dir: &Path) -> Result<HashSet<String>> {
    let path = state_dir.join(FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", path.display()));
        }
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Save the set atomically: write a temporary file, sync it, then rename it
/// over the real one. A failed write leaves the previous state untouched.
pub fn save(state_dir: &Path, seen: &HashSet<String>) -> Result<()> {
    fs::create_dir_all(state_dir)
        .with_context(|| format!("creating state dir {}", state_dir.display()))?;

    let path = state_dir.join(FILE);
    let temp = state_dir.join(TEMP);
    let result = write_and_rename(&temp, &path, seen);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.with_context(|| format!("saving state to {}", path.display()))
}

fn write_and_rename(temp: &Path, path: &Path, seen: &HashSet<String>) -> Result<()> {
    let mut file = File::create(temp)?;
    for url in seen {
        writeln!(file, "{url}")?;
    }
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("olx-scraper-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips() {
        let dir = temp_dir("round-trip");
        let seen: HashSet<String> = ["https://example/one", "https://example/two"]
            .into_iter()
            .map(str::to_owned)
            .collect();

        save(&dir, &seen).unwrap();

        assert_eq!(load(&dir).unwrap(), seen);
        assert!(!dir.join(TEMP).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replaces_previous_state() {
        let dir = temp_dir("replace");
        save(
            &dir,
            &["https://example/old".to_owned()].into_iter().collect(),
        )
        .unwrap();
        save(&dir, &HashSet::new()).unwrap();

        assert!(load(&dir).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_state_is_empty() {
        let dir = temp_dir("missing");
        assert!(load(&dir).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
