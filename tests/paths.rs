//! Locations live in src/paths.rs only.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

#[test]
fn no_literal_paths_outside_paths_rs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("tests"), &mut files);
    let banned = ["/home/", "from(\"data\")", "\"data/", "Obsidian"];
    let mut hits = Vec::new();
    for f in files {
        let rel = f.strip_prefix(root).unwrap().to_string_lossy().to_string();
        if rel == "src/paths.rs" || rel == "tests/paths.rs" {
            continue;
        }
        for (n, line) in fs::read_to_string(&f).unwrap().lines().enumerate() {
            if banned.iter().any(|b| line.contains(b)) {
                hits.push(format!("{rel}:{}", n + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "route these through src/paths.rs: {hits:?}"
    );
}

#[test]
fn default_is_todays_relative_data_dir() {
    if std::env::var_os("CHESS_PUZZLES_DATA").is_none() {
        assert_eq!(
            chess_puzzles_tui::paths::pack_file(),
            Path::new("data").join("puzzles.json")
        );
    }
}
