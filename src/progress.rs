//! Saved attempt history (`data/progress.json`).
//!
//! The on-disk document is a JSON object with `version` 1 and an `attempts`
//! array. A missing file is an empty history. A file that cannot be read or
//! parsed is renamed aside and treated as empty so a later save does not
//! destroy it.

use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::Grade;

const FILE_VERSION: u32 = 1;

/// Points for one graded attempt.
///
/// Solved with no hint: mate in 1 is 3, mate in 2 is 5, mate in 3 is 8.
/// Second try with no hint is half of that score, rounded down
/// (mate in 1 is 1, mate in 2 is 2, mate in 3 is 4).
/// A hint grade, or a hint used on a solved or second-try attempt, is 1.
/// Missed is 0.
pub fn points(mate_in: u8, grade: Grade, hint_used: bool) -> u32 {
    match (grade, hint_used) {
        (Grade::Missed, _) => 0,
        (Grade::Hint, _) | (Grade::Solved, true) | (Grade::SecondTry, true) => 1,
        (Grade::Solved, false) => full_score(mate_in),
        (Grade::SecondTry, false) => full_score(mate_in) / 2,
    }
}

fn full_score(mate_in: u8) -> u32 {
    match mate_in {
        1 => 3,
        2 => 5,
        3 => 8,
        _ => 0,
    }
}

/// Grade string stored in the history file.
pub(crate) fn grade_key(grade: Grade) -> &'static str {
    match grade {
        Grade::Solved => "solved",
        Grade::SecondTry => "second",
        Grade::Hint => "hint",
        Grade::Missed => "missed",
    }
}

/// One graded attempt, in memory and on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub at: String,
    pub puzzle_id: String,
    pub mate_in: u8,
    pub grade: String,
    pub hint_used: bool,
    pub elapsed_secs: u64,
    pub timer_secs: u64,
    pub points: u32,
}

/// Counts and points over a set of attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Totals {
    pub solved: u32,
    pub second: u32,
    pub hint: u32,
    pub missed: u32,
    pub attempts: u32,
    pub points: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProgressFile {
    version: u32,
    attempts: Vec<Record>,
}

/// Attempts loaded from a progress file.
#[derive(Debug, Default)]
pub struct History {
    attempts: Vec<Record>,
    warning: Option<String>,
    /// The previous file could not be moved aside. Further saves would
    /// overwrite it, so they are refused.
    save_blocked: bool,
}

impl History {
    /// Load history from `path`.
    ///
    /// A missing file is an empty history. A file that cannot be read, or
    /// that is not version-1 JSON, is renamed to
    /// `progress.corrupt-<YYYYMMDD-HHMMSS>.json` in the same directory
    /// (using the file's own stem) and this returns an empty history with
    /// a warning. This never panics.
    pub fn load(path: &Path) -> History {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == ErrorKind::NotFound => return History::default(),
            Err(_) => return quarantine(path),
        };
        match serde_json::from_str::<ProgressFile>(&raw) {
            Ok(file) if file.version == FILE_VERSION => History {
                attempts: file.attempts,
                warning: None,
                save_blocked: false,
            },
            _ => quarantine(path),
        }
    }

    /// Push `record` and write the whole history to `path`.
    ///
    /// The parent directory is created when it is missing. JSON is written
    /// to a sibling temp file (`progress.json.tmp` for `progress.json`),
    /// flushed and synced, then renamed over `path`. On error the record
    /// stays in memory.
    pub fn append(&mut self, path: &Path, record: Record) -> Result<(), String> {
        self.attempts.push(record);
        if self.save_blocked {
            return Err("progress file was not saved".to_string());
        }
        self.write_file(path)
    }

    pub fn attempts(&self) -> &[Record] {
        &self.attempts
    }

    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    pub fn totals(&self) -> Totals {
        totals_of(self.attempts.iter())
    }

    pub fn totals_for(&self, mate_in: u8) -> Totals {
        totals_of(
            self.attempts
                .iter()
                .filter(move |record| record.mate_in == mate_in),
        )
    }

    fn write_file(&self, path: &Path) -> Result<(), String> {
        let file = ProgressFile {
            version: FILE_VERSION,
            attempts: self.attempts.clone(),
        };
        let mut json = serde_json::to_string_pretty(&file)
            .map_err(|err| format!("cannot encode progress: {err}"))?;
        json.push('\n');
        write_atomic(path, json.as_bytes())
    }
}

fn totals_of<'a>(records: impl Iterator<Item = &'a Record>) -> Totals {
    let mut totals = Totals::default();
    for record in records {
        match record.grade.as_str() {
            "solved" => totals.solved += 1,
            "second" => totals.second += 1,
            "hint" => totals.hint += 1,
            "missed" => totals.missed += 1,
            _ => {}
        }
        totals.attempts += 1;
        totals.points += record.points;
    }
    totals
}

fn quarantine(path: &Path) -> History {
    match move_aside(path) {
        Ok(name) => History {
            attempts: Vec::new(),
            warning: Some(format!("bad progress file renamed to {name}")),
            save_blocked: false,
        },
        Err(_) => History {
            attempts: Vec::new(),
            warning: Some("bad progress file could not be moved aside".to_string()),
            save_blocked: true,
        },
    }
}

fn move_aside(path: &Path) -> Result<String, String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("progress");
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    for n in 0..1000 {
        let name = if n == 0 {
            format!("{stem}.corrupt-{stamp}.json")
        } else {
            format!("{stem}.corrupt-{stamp}-{n}.json")
        };
        let dest = sibling(path, &name);
        if dest.exists() {
            continue;
        }
        match std::fs::rename(path, &dest) {
            Ok(()) => return Ok(name),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.to_string()),
        }
    }
    Err("could not name a copy of the progress file".to_string())
}

/// Flush and sync a temp file in the same directory, then rename it over `path`.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
        }
    }
    let tmp = tmp_path_for(path);
    if let Err(err) = write_synced(&tmp, bytes) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("cannot replace {}: {err}", path.display()));
    }
    Ok(())
}

fn write_synced(tmp: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file =
        File::create(tmp).map_err(|err| format!("cannot write {}: {err}", tmp.display()))?;
    file.write_all(bytes)
        .map_err(|err| format!("cannot write {}: {err}", tmp.display()))?;
    file.flush()
        .map_err(|err| format!("cannot flush {}: {err}", tmp.display()))?;
    file.sync_all()
        .map_err(|err| format!("cannot sync {}: {err}", tmp.display()))?;
    Ok(())
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("progress.json");
    sibling(path, &format!("{file_name}.tmp"))
}

fn sibling(path: &Path, file_name: &str) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

/// Progress path relative to the CWD (`./play` cds to the project root).
pub fn progress_path() -> PathBuf {
    PathBuf::from("data").join("progress.json")
}
