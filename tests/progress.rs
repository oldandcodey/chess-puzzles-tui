use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chess_puzzles_tui::app::{App, Grade, Phase, Screen};
use chess_puzzles_tui::pack::{pack_path, Pack};
use chess_puzzles_tui::progress::{points, progress_path, History, Record, Totals};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn pack() -> Pack {
    Pack::load(&pack_path()).expect("data/puzzles.json loads")
}

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "chess-puzzles-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn record(mate_in: u8, grade: &str, hint_used: bool, scored: u32) -> Record {
    Record {
        at: "2026-10-09T12:00:00-07:00".to_string(),
        puzzle_id: format!("p{mate_in}-{grade}"),
        mate_in,
        grade: grade.to_string(),
        hint_used,
        elapsed_secs: 15,
        timer_secs: 60,
        points: scored,
    }
}

fn is_rfc3339(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() < 19 {
        return false;
    }
    let digit = |i: usize| b[i].is_ascii_digit();
    digit(0)
        && digit(1)
        && digit(2)
        && digit(3)
        && b[4] == b'-'
        && digit(5)
        && digit(6)
        && b[7] == b'-'
        && digit(8)
        && digit(9)
        && b[10] == b'T'
        && digit(11)
        && digit(12)
        && b[13] == b':'
        && digit(14)
        && digit(15)
        && b[16] == b':'
        && digit(17)
        && digit(18)
        && (value.contains('+') || value.ends_with('Z') || value.matches('-').count() >= 3)
}

fn is_corrupt_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("progress.corrupt-") else {
        return false;
    };
    let Some(rest) = rest.strip_suffix(".json") else {
        return false;
    };
    let b = rest.as_bytes();
    if b.len() < 15 {
        return false;
    }
    let stamp = b[..8].iter().all(|c| c.is_ascii_digit())
        && b[8] == b'-'
        && b[9..15].iter().all(|c| c.is_ascii_digit());
    let suffix = b.len() == 15
        || (b[15] == b'-' && b.len() > 16 && b[16..].iter().all(|c| c.is_ascii_digit()));
    stamp && suffix
}

fn assert_no_tmp(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(!name.ends_with(".tmp"), "leftover temp file {name}");
        if entry.path().is_dir() {
            assert_no_tmp(&entry.path());
        }
    }
}

fn render(app: &App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).expect("test terminal");
    term.draw(|f| chess_puzzles_tui::ui::draw(f, app))
        .expect("draw");
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn points_cover_every_grade_level_and_hint() {
    let cases = [
        (1, Grade::Solved, false, 3),
        (2, Grade::Solved, false, 5),
        (3, Grade::Solved, false, 8),
        (1, Grade::Solved, true, 1),
        (2, Grade::Solved, true, 1),
        (3, Grade::Solved, true, 1),
        (1, Grade::SecondTry, false, 1),
        (2, Grade::SecondTry, false, 2),
        (3, Grade::SecondTry, false, 4),
        (1, Grade::SecondTry, true, 1),
        (2, Grade::SecondTry, true, 1),
        (3, Grade::SecondTry, true, 1),
        (1, Grade::Hint, false, 1),
        (2, Grade::Hint, false, 1),
        (3, Grade::Hint, false, 1),
        (1, Grade::Hint, true, 1),
        (2, Grade::Hint, true, 1),
        (3, Grade::Hint, true, 1),
        (1, Grade::Missed, false, 0),
        (2, Grade::Missed, false, 0),
        (3, Grade::Missed, false, 0),
        (1, Grade::Missed, true, 0),
        (2, Grade::Missed, true, 0),
        (3, Grade::Missed, true, 0),
    ];
    assert_eq!(cases.len(), 24);
    for (mate_in, grade, hint_used, expect) in cases {
        assert_eq!(
            points(mate_in, grade, hint_used),
            expect,
            "mate {mate_in} {grade:?} hint {hint_used}"
        );
    }
}

#[test]
fn progress_path_is_beside_the_pack() {
    assert_eq!(progress_path(), PathBuf::from("data").join("progress.json"));
}

#[test]
fn missing_file_is_an_empty_history() {
    let tmp = Tmp::new("missing");
    let path = tmp.path().join("progress.json");
    let history = History::load(&path);
    assert!(history.attempts().is_empty());
    assert!(history.warning().is_none());
    assert_eq!(history.totals().attempts, 0);
    assert_eq!(history.totals().points, 0);
    assert!(!path.exists());
}

#[test]
fn save_and_load_round_trip_leaves_no_tmp() {
    let tmp = Tmp::new("round");
    let path = tmp.path().join("nested").join("progress.json");
    let mut history = History::load(&path);
    assert!(history.attempts().is_empty());

    let first = record(2, "second", false, points(2, Grade::SecondTry, false));
    let second = record(3, "solved", false, points(3, Grade::Solved, false));
    history.append(&path, first.clone()).expect("first save");
    history.append(&path, second.clone()).expect("second save");
    assert_no_tmp(tmp.path());

    let text = std::fs::read_to_string(&path).expect("read back");
    assert!(text.starts_with("{\n"), "{text}");
    assert!(text.contains("\"version\": 1"), "{text}");
    assert!(text.contains("\"attempts\""), "{text}");
    assert!(text.contains("\"puzzle_id\""), "{text}");
    assert!(text.contains("\"mate_in\""), "{text}");
    assert!(text.contains("\"hint_used\""), "{text}");
    assert!(text.contains("\"elapsed_secs\""), "{text}");
    assert!(text.contains("\"timer_secs\""), "{text}");
    assert!(text.contains("\"points\""), "{text}");
    assert!(text.ends_with("}\n"), "{text}");

    let loaded = History::load(&path);
    assert!(loaded.warning().is_none());
    assert_eq!(loaded.attempts(), &[first, second]);
    assert_eq!(loaded.totals().attempts, 2);
    assert_eq!(loaded.totals().points, 2 + 8);
}

#[test]
fn corrupt_file_is_renamed_and_a_later_append_writes_a_valid_file() {
    let tmp = Tmp::new("corrupt");
    let path = tmp.path().join("progress.json");
    let original = b"{\"version\":1,\"attempts\":[";
    std::fs::write(&path, original).unwrap();

    let mut history = History::load(&path);
    assert!(history.attempts().is_empty());
    let warning = history.warning().expect("warning");
    assert!(!path.exists(), "bad file should have been moved aside");

    let names: Vec<String> = std::fs::read_dir(tmp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let corrupt: Vec<_> = names
        .iter()
        .filter(|name| name.contains("corrupt"))
        .cloned()
        .collect();
    assert_eq!(corrupt.len(), 1, "{names:?}");
    assert!(is_corrupt_name(&corrupt[0]), "{}", corrupt[0]);
    assert!(warning.contains(&corrupt[0]), "{warning}");
    assert_eq!(
        std::fs::read(tmp.path().join(&corrupt[0])).unwrap(),
        original
    );

    let row = record(1, "solved", false, 3);
    history
        .append(&path, row.clone())
        .expect("append after quarantine");
    assert_no_tmp(tmp.path());
    assert!(path.is_file());
    assert_eq!(
        std::fs::read(tmp.path().join(&corrupt[0])).unwrap(),
        original,
        "quarantined bytes stay intact after the new file is written"
    );

    let loaded = History::load(&path);
    assert!(loaded.warning().is_none());
    assert_eq!(loaded.attempts(), &[row]);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"version\": 1"), "{text}");
}

#[test]
fn a_different_version_is_renamed_aside() {
    let tmp = Tmp::new("version");
    let path = tmp.path().join("progress.json");
    let original = b"{\"version\":2,\"attempts\":[]}\n";
    std::fs::write(&path, original).unwrap();
    let history = History::load(&path);
    assert!(history.attempts().is_empty());
    assert!(history.warning().is_some());
    assert!(!path.exists());
    let aside = std::fs::read_dir(tmp.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("corrupt"))
        })
        .expect("renamed aside");
    assert_eq!(std::fs::read(&aside).unwrap(), original);
}

#[test]
fn append_keeps_the_record_when_the_save_fails() {
    let tmp = Tmp::new("save-fail");
    let blocker = tmp.path().join("not-a-directory");
    std::fs::write(&blocker, b"x").unwrap();
    let path = blocker.join("progress.json");
    let mut history = History::load(&tmp.path().join("missing.json"));
    let row = record(1, "hint", true, 1);
    let err = history.append(&path, row.clone()).expect_err("save fails");
    assert!(!err.is_empty());
    assert_eq!(history.attempts(), &[row]);
    assert!(!path.exists());
}

#[test]
fn totals_are_split_by_mate_in_level() {
    let tmp = Tmp::new("totals");
    let path = tmp.path().join("progress.json");
    let mut history = History::load(&path);
    let rows = [
        record(1, "solved", false, 3),
        record(1, "second", false, 1),
        record(1, "missed", true, 0),
        record(2, "solved", true, 1),
        record(2, "hint", false, 1),
        record(3, "second", false, 4),
        record(3, "solved", false, 8),
    ];
    for row in rows.iter().cloned() {
        history.append(&path, row).unwrap();
    }
    let loaded = History::load(&path);
    assert_eq!(loaded.totals(), history.totals());

    assert_eq!(
        loaded.totals(),
        Totals {
            solved: 3,
            second: 2,
            hint: 1,
            missed: 1,
            attempts: 7,
            points: 18,
        }
    );

    assert_eq!(
        loaded.totals_for(1),
        Totals {
            solved: 1,
            second: 1,
            hint: 0,
            missed: 1,
            attempts: 3,
            points: 4,
        }
    );
    assert_eq!(
        loaded.totals_for(2),
        Totals {
            solved: 1,
            second: 0,
            hint: 1,
            missed: 0,
            attempts: 2,
            points: 2,
        }
    );
    assert_eq!(
        loaded.totals_for(3),
        Totals {
            solved: 1,
            second: 1,
            hint: 0,
            missed: 0,
            attempts: 2,
            points: 12,
        }
    );
    assert_eq!(loaded.totals_for(9), Totals::default());
}

#[test]
fn grading_writes_one_row_and_escape_writes_nothing() {
    let tmp = Tmp::new("grade");
    let path = tmp.path().join("progress.json");
    let mut app = App::with_progress(pack(), path.clone());
    app.on_char('e');
    assert_eq!(app.screen, Screen::Endings);
    let id = app.current().unwrap().id.clone();
    assert_eq!(app.current().unwrap().mate_in, 1);

    app.enter();
    assert_eq!(app.phase, Phase::Solving);
    app.on_char('h');
    app.back();
    assert_eq!(app.phase, Phase::Setup);
    assert!(app.attempts.is_empty());
    assert!(app.history().attempts().is_empty());
    assert!(!path.exists());

    app.enter();
    app.on_char('r');
    assert_eq!(app.phase, Phase::Review);
    app.back();
    assert_eq!(app.phase, Phase::Setup);
    assert!(app.attempts.is_empty());
    assert!(!path.exists());
    assert_eq!(app.current().unwrap().id, id);

    app.enter();
    app.on_char('r');
    app.on_char('y');
    assert_eq!(app.attempts.len(), 1);
    assert_eq!(app.attempts[0].puzzle_id, id);
    assert_eq!(app.attempts[0].grade, Grade::Solved);
    assert!(!app.attempts[0].hint_used);
    assert_eq!(app.attempts[0].points, 3);
    assert_eq!(app.attempts[0].timer_secs, 60);
    assert!(app.save_warning.is_none());
    assert_eq!(app.history().attempts().len(), 1);

    let loaded = History::load(&path);
    assert_eq!(loaded.attempts().len(), 1);
    assert_eq!(loaded.attempts()[0].puzzle_id, id);
    assert_eq!(loaded.attempts()[0].grade, "solved");
    assert_eq!(loaded.attempts()[0].mate_in, 1);
    assert!(!loaded.attempts()[0].hint_used);
    assert_eq!(loaded.attempts()[0].points, 3);
    assert_eq!(loaded.attempts()[0].timer_secs, 60);
    assert_eq!(
        loaded.attempts()[0].elapsed_secs,
        app.attempts[0].elapsed_secs
    );
    assert!(
        is_rfc3339(&loaded.attempts()[0].at),
        "{}",
        loaded.attempts()[0].at
    );
    assert_no_tmp(tmp.path());

    app.on_char('2');
    assert_eq!(app.level, 2);
    assert_eq!(app.phase, Phase::Setup);
    app.enter();
    app.on_char('r');
    app.on_char('2');
    assert_eq!(app.attempts[1].mate_in, 2);
    assert_eq!(app.attempts[1].grade, Grade::SecondTry);
    assert!(!app.attempts[1].hint_used);
    assert_eq!(app.attempts[1].points, 2);
    assert_eq!(app.attempts[1].timer_secs, 180);

    app.on_char('3');
    app.enter();
    app.on_char('h');
    app.on_char('r');
    app.on_char('n');
    assert_eq!(app.attempts[2].mate_in, 3);
    assert_eq!(app.attempts[2].grade, Grade::Missed);
    assert!(app.attempts[2].hint_used);
    assert_eq!(app.attempts[2].points, 0);

    let loaded = History::load(&path);
    assert_eq!(loaded.attempts().len(), 3);
    assert_eq!(loaded.attempts()[1].grade, "second");
    assert_eq!(loaded.attempts()[1].points, 2);
    assert_eq!(loaded.attempts()[1].mate_in, 2);
    assert_eq!(loaded.attempts()[2].grade, "missed");
    assert_eq!(loaded.attempts()[2].points, 0);
    assert!(loaded.attempts()[2].hint_used);
    assert_eq!(loaded.totals().points, 5);
    assert_eq!(loaded.totals_for(1).points, 3);
    assert_eq!(loaded.totals_for(2).points, 2);
    assert_eq!(loaded.totals_for(3).points, 0);
    assert_eq!(app.session_counts().solved, 1);
    assert_eq!(app.session_counts().second, 1);
    assert_eq!(app.session_counts().missed, 1);
}

#[test]
fn app_new_does_not_touch_the_progress_file() {
    let path = progress_path();
    let before = std::fs::read(&path).ok();
    let mut app = App::new(pack());
    assert!(app.history().attempts().is_empty());
    assert!(app.history().warning().is_none());
    assert!(app.save_warning.is_none());
    app.on_char('e');
    app.enter();
    app.on_char('r');
    app.on_char('y');
    assert_eq!(app.attempts.len(), 1);
    assert_eq!(app.attempts[0].points, 3);
    assert!(app.history().attempts().is_empty());
    let after = std::fs::read(&path).ok();
    assert_eq!(before, after);
}

#[test]
fn progress_screen_fits_an_80x24_terminal() {
    let tmp = Tmp::new("screen");
    let path = tmp.path().join("progress.json");
    let mut history = History::load(&path);
    history
        .append(
            &path,
            Record {
                at: "2026-10-09T12:00:00-07:00".to_string(),
                puzzle_id: "wide".to_string(),
                mate_in: 1,
                grade: "solved".to_string(),
                hint_used: false,
                elapsed_secs: 1,
                timer_secs: 60,
                points: 9999,
            },
        )
        .unwrap();

    let mut app = App::with_progress(pack(), path);
    app.save_warning = Some("cannot save progress".to_string());
    app.on_char('p');
    assert_eq!(app.screen, Screen::Progress);
    let screen = render(&app, 80, 24);
    assert!(screen.contains("All time (data/progress.json)"), "{screen}");
    assert!(screen.contains("This session"), "{screen}");
    assert!(screen.contains("cannot save progress"), "{screen}");
    assert!(!screen.contains("week 3"), "{screen}");
    assert!(!screen.contains("not saved"), "{screen}");
    for level in 1..=3 {
        let label = format!("Mate in {level}");
        let lines: Vec<_> = screen
            .lines()
            .filter(|line| line.contains(&label))
            .collect();
        assert!(lines.len() >= 2, "missing {label}\n{screen}");
        assert!(
            lines
                .iter()
                .all(|line| line.contains("attempts") && line.contains("points")),
            "{label} line dropped a column\n{screen}"
        );
    }
    assert!(
        screen
            .lines()
            .any(|line| line.contains("Mate in 1") && line.contains("points 9999")),
        "{screen}"
    );

    let bad = tmp.path().join("broken.json");
    std::fs::write(&bad, b"NOPE").unwrap();
    let mut broken = App::with_progress(pack(), bad);
    let warning = broken.history().warning().unwrap().to_string();
    broken.on_char('p');
    let screen = render(&broken, 80, 24);
    assert!(screen.contains(&warning), "{screen}");
    assert!(screen.contains("This session"), "{screen}");
    assert!(screen.contains("Mate in 3"), "{screen}");
}
