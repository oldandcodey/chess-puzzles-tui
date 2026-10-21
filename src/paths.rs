//! The only place that knows where the game's files live. `$CHESS_PUZZLES_DATA` overrides the
//! data folder; the default is today's `data/` relative to the working directory (`./play` cds
//! to the project root). `tests/paths.rs` fails on a literal data or /home/ path anywhere else.

use std::env;
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    env::var("CHESS_PUZZLES_DATA")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"))
}

pub fn pack_file() -> PathBuf {
    data_dir().join("puzzles.json")
}

pub fn progress_file() -> PathBuf {
    data_dir().join("progress.json")
}
