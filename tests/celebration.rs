//! When the clean-solve celebration fires, and when it does not.

use std::time::Duration;

use chess_puzzles_tui::app::{is_clean_solve, App, Grade, Phase, Screen, CELEBRATION_DURATION};
use chess_puzzles_tui::pack::{pack_path, Pack};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn pack() -> Pack {
    Pack::load(&pack_path()).expect("puzzle pack loads")
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

fn reveal_and(app: &mut App, hint: bool, key: char) {
    app.enter();
    assert_eq!(app.phase, Phase::Solving);
    if hint {
        app.on_char('h');
    }
    app.on_char('r');
    app.on_char(key);
}

#[test]
fn only_a_clean_solve_celebrates() {
    let cases = [
        (Grade::Solved, false, true),
        (Grade::Solved, true, false),
        (Grade::SecondTry, false, false),
        (Grade::SecondTry, true, false),
        (Grade::Hint, false, false),
        (Grade::Hint, true, false),
        (Grade::Missed, false, false),
        (Grade::Missed, true, false),
    ];
    for (grade, hint_used, expect) in cases {
        assert_eq!(
            is_clean_solve(grade, hint_used),
            expect,
            "{grade:?} hint_used={hint_used}"
        );
    }
}

#[test]
fn celebration_stays_up_for_about_a_second_and_a_half() {
    assert!(CELEBRATION_DURATION >= Duration::from_secs(1));
    assert!(CELEBRATION_DURATION <= Duration::from_secs(2));
}

#[test]
fn the_drill_celebrates_only_a_clean_solve() {
    let mut app = App::new(pack());
    app.on_char('e');
    let solved_id = app.current().unwrap().id.clone();

    reveal_and(&mut app, false, 'y');
    assert!(app.showing_celebration(), "Y with no hint");
    assert_eq!(app.phase, Phase::Setup);
    assert_ne!(app.current().unwrap().id, solved_id);
    assert_eq!(app.attempts.last().unwrap().grade, Grade::Solved);
    assert!(!app.attempts.last().unwrap().hint_used);
    assert_eq!(app.attempts.last().unwrap().points, 3);

    let screen = render(&app, 80, 24);
    assert!(screen.contains("CLEAN SOLVE"), "{screen}");
    assert!(screen.contains(&solved_id), "{screen}");
    assert!(screen.contains("mate in 1"), "{screen}");
    assert!(screen.contains("+3"), "{screen}");
    assert!(screen.contains("any key skips"), "{screen}");
    assert!(
        !screen.contains("2/100"),
        "the next puzzle stays hidden until the flash ends\n{screen}"
    );
    assert!(screen.is_ascii(), "celebration must stay fixed-width ascii");
    for line in screen.lines() {
        assert!(
            line.chars().count() <= 80,
            "80 columns: {line} ({})",
            line.chars().count()
        );
    }
    // Short frames must still draw.
    let _ = render(&app, 40, 16);
    let _ = render(&app, 10, 8);

    let puzzle_id = app.current().unwrap().id.clone();
    app.on_char('x');
    assert!(!app.showing_celebration(), "an unbound key skips");
    assert_eq!(app.level, 1);
    assert_eq!(app.phase, Phase::Setup);
    assert_eq!(app.current().unwrap().id, puzzle_id);

    reveal_and(&mut app, false, '2');
    assert!(!app.showing_celebration(), "second try");
    assert_eq!(app.attempts.last().unwrap().grade, Grade::SecondTry);
    assert_eq!(app.attempts.last().unwrap().points, 1);
    let second = render(&app, 80, 24);
    assert!(!second.contains("CLEAN SOLVE"), "{second}");

    reveal_and(&mut app, true, '2');
    assert!(!app.showing_celebration(), "second try after a hint");
    assert!(app.attempts.last().unwrap().hint_used);
    assert_eq!(app.attempts.last().unwrap().points, 1);

    reveal_and(&mut app, false, 'h');
    assert!(!app.showing_celebration(), "hint grade");
    assert_eq!(app.attempts.last().unwrap().grade, Grade::Hint);
    assert_eq!(app.attempts.last().unwrap().points, 1);

    reveal_and(&mut app, true, 'h');
    assert!(!app.showing_celebration(), "hint grade after H");

    reveal_and(&mut app, false, 'n');
    assert!(!app.showing_celebration(), "miss");
    assert_eq!(app.attempts.last().unwrap().grade, Grade::Missed);
    assert_eq!(app.attempts.last().unwrap().points, 0);

    reveal_and(&mut app, true, 'n');
    assert!(!app.showing_celebration(), "miss after a hint");
    assert_eq!(app.attempts.last().unwrap().points, 0);

    reveal_and(&mut app, true, 'y');
    assert!(
        !app.showing_celebration(),
        "Y after a hint during the attempt is not clean"
    );
    assert_eq!(app.attempts.last().unwrap().grade, Grade::Solved);
    assert!(app.attempts.last().unwrap().hint_used);
    assert_eq!(app.attempts.last().unwrap().points, 1);
}

#[test]
fn celebration_ends_on_a_timer_and_can_be_turned_off() {
    let mut app = App::new(pack());
    app.on_char('e');
    reveal_and(&mut app, false, 'y');
    let deadline = app.celebration_deadline().expect("a deadline");
    app.tick_at(deadline - Duration::from_millis(1));
    assert!(app.showing_celebration());
    app.tick_at(deadline);
    assert!(!app.showing_celebration());
    let screen = render(&app, 80, 24);
    assert!(!screen.contains("CLEAN SOLVE"), "{screen}");
    assert!(screen.contains("2/100"), "{screen}");
    assert!(screen.contains("timer"), "{screen}");

    let mut quiet = App::new(pack());
    quiet.set_celebrations_enabled(false);
    quiet.on_char('e');
    reveal_and(&mut quiet, false, 'y');
    assert!(!quiet.showing_celebration());
    assert_eq!(quiet.phase, Phase::Setup);
    assert_eq!(quiet.attempts.last().unwrap().points, 3);
    let quiet_screen = render(&quiet, 80, 24);
    assert!(!quiet_screen.contains("CLEAN SOLVE"), "{quiet_screen}");
    assert!(quiet_screen.contains("2/100"), "{quiet_screen}");

    let mut flashed = App::new(pack());
    flashed.on_char('e');
    reveal_and(&mut flashed, false, 'y');
    assert!(flashed.showing_celebration());
    flashed.set_celebrations_enabled(false);
    assert!(!flashed.showing_celebration());
}

#[test]
fn arrows_and_escape_skip_without_eating_the_key() {
    let mut app = App::new(pack());
    app.on_char('e');
    reveal_and(&mut app, false, 'y');
    let puzzle_id = app.current().unwrap().id.clone();
    app.up();
    assert!(!app.showing_celebration());
    assert_eq!(app.screen, Screen::Endings);
    assert_eq!(app.current().unwrap().id, puzzle_id);

    reveal_and(&mut app, false, 'y');
    assert!(app.showing_celebration());
    app.back();
    assert!(!app.showing_celebration());
    assert_eq!(
        app.screen,
        Screen::Menu,
        "Esc on the flash still leaves setup, same as any other setup screen"
    );
}
