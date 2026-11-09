//! Solution entry: the move box, Tab, and which keys do what in each phase.

use chess_puzzles_tui::app::{App, Grade, Phase};
use chess_puzzles_tui::moves::{Feedback, MoveMark};
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

fn endings_mate_in_2(app: &mut App) {
    app.on_char('e');
    app.on_char('2');
    assert_eq!(app.current().unwrap().id, "02Ofv");
}

fn type_move(app: &mut App, text: &str) {
    while !app.move_entry().buffer().is_empty() {
        app.backspace();
    }
    for ch in text.chars() {
        app.on_char(ch);
    }
    app.enter();
}

#[test]
fn setup_keys_stay_commands_and_the_box_turns_on_with_the_clock() {
    let mut app = App::new(pack());
    endings_mate_in_2(&mut app);
    let first = app.current().unwrap().id.clone();

    app.on_char('n');
    assert_ne!(app.current().unwrap().id, first);
    assert_eq!(app.move_entry().buffer(), "");
    app.on_char('b');
    assert_eq!(app.current().unwrap().id, first);
    app.on_char('+');
    assert_eq!(app.timer_secs, 210);
    app.on_char('1');
    assert_eq!(app.level, 1);
    assert_eq!(app.phase, Phase::Setup);
    app.on_char('2');
    assert_eq!(app.level, 2);

    assert!(!app.pieces_shown());
    app.toggle_pieces();
    assert!(app.pieces_shown());
    let listed = render(&app, 100, 30);
    assert!(listed.contains("White"), "{listed}");
    assert!(listed.contains("Move"), "{listed}");
    app.toggle_pieces();
    let hidden = render(&app, 100, 30);
    assert!(!hidden.contains("White  K"), "{hidden}");
    assert!(hidden.contains("Tab pieces"), "{hidden}");
    assert!(hidden.contains("(none yet)"), "{hidden}");

    app.on_char(' ');
    assert_eq!(app.phase, Phase::Solving);
    assert!(app.field_active());
    type_move(&mut app, "n");
    assert_eq!(app.level, 2, "n is a knight move, not the next puzzle");
    assert_eq!(app.current().unwrap().id, first);
    assert_eq!(app.move_entry().log()[0].mark, MoveMark::Illegal);
    assert_eq!(app.move_entry().feedback(), &Feedback::Illegal);

    app.backspace();
    assert!(!app.move_entry().rejected());
    app.hint();
    assert!(app.hint_used);
    assert_eq!(app.phase, Phase::Solving);
    app.reveal();
    assert_eq!(app.phase, Phase::Review);
    assert!(!app.field_active());
    app.on_char('y');
    assert_eq!(app.attempts.last().unwrap().grade, Grade::Solved);
    assert!(app.attempts.last().unwrap().hint_used);
    assert!(!app.showing_celebration(), "a hinted solve is not clean");
}

#[test]
fn tab_toggles_in_every_phase_and_does_not_skip_the_celebration() {
    let mut app = App::new(pack());
    endings_mate_in_2(&mut app);
    assert!(!app.pieces_shown());
    app.toggle_pieces();
    app.enter();
    assert!(app.pieces_shown());
    app.toggle_pieces();
    assert!(!app.pieces_shown());
    assert_eq!(app.phase, Phase::Solving);

    app.reveal();
    app.on_char('y');
    assert!(app.showing_celebration());
    let before = app.pieces_shown();
    app.toggle_pieces();
    assert!(app.showing_celebration(), "Tab does not dismiss the flash");
    assert_eq!(app.pieces_shown(), !before);
    app.on_char('x');
    assert!(!app.showing_celebration());
    assert_eq!(app.pieces_shown(), !before, "the toggle survives the flash");
}

#[test]
fn typing_02ofv_plays_the_reply_and_a_clean_solve_celebrates() {
    let mut app = App::new(pack());
    endings_mate_in_2(&mut app);
    let id = app.current().unwrap().id.clone();
    app.enter();

    type_move(&mut app, "Rc1+");
    assert_eq!(app.move_entry().log()[0].mark, MoveMark::Correct);
    assert!(app.move_entry().waiting_for_reply());
    let during = app.move_entry().fen();
    assert!(during.contains("2r3KR"), "{during}");

    let deadline = app.move_entry().reply_deadline().unwrap();
    app.tick_at(deadline);
    let reply = app.move_entry().reply().unwrap();
    assert_eq!(reply.side, "White");
    assert_eq!(reply.san, "Re1");
    assert!(app.field_active());

    type_move(&mut app, "Kb6");
    assert_eq!(app.move_entry().log()[1].mark, MoveMark::Wrong);
    assert_eq!(app.move_entry().feedback(), &Feedback::Wrong);
    assert!(app.move_entry().reply().is_some(), "the reply note stays");
    let after_wrong = app.move_entry().fen();
    assert!(after_wrong.contains("2r1R1KR"), "{after_wrong}");

    // Illegal on the same position leaves the board where the reply put it.
    type_move(&mut app, "Nf3");
    assert_eq!(app.move_entry().log()[2].mark, MoveMark::Illegal);
    assert_eq!(app.move_entry().feedback(), &Feedback::Illegal);
    assert_eq!(app.move_entry().fen(), after_wrong);

    type_move(&mut app, "Rxe1#");
    assert_eq!(app.attempts.last().unwrap().puzzle_id, id);
    assert_eq!(app.attempts.last().unwrap().grade, Grade::SecondTry);
    assert!(
        !app.showing_celebration(),
        "a wrong try is not a clean solve"
    );
    assert_eq!(app.phase, Phase::Setup);
}

#[test]
fn a_typed_mate_with_no_wrong_move_celebrates_unless_disabled() {
    let mut app = App::new(pack());
    endings_mate_in_2(&mut app);
    app.enter();
    type_move(&mut app, "Rc1+");
    app.tick_at(app.move_entry().reply_deadline().unwrap());
    type_move(&mut app, "c1e1");
    assert_eq!(app.attempts.last().unwrap().grade, Grade::Solved);
    assert!(!app.attempts.last().unwrap().hint_used);
    assert_eq!(app.attempts.last().unwrap().points, 5);
    assert!(app.showing_celebration());
    let screen = render(&app, 100, 30);
    assert!(screen.contains("CLEAN SOLVE"), "{screen}");
    assert!(screen.contains("02Ofv"), "{screen}");
    assert!(screen.contains("Tab pieces"), "{screen}");

    let mut quiet = App::new(pack());
    quiet.set_celebrations_enabled(false);
    endings_mate_in_2(&mut quiet);
    quiet.enter();
    type_move(&mut quiet, "Rc1+");
    quiet.tick_at(quiet.move_entry().reply_deadline().unwrap());
    type_move(&mut quiet, "Rxe1#");
    assert_eq!(quiet.attempts.last().unwrap().grade, Grade::Solved);
    assert!(!quiet.showing_celebration());
}

#[test]
fn the_screen_marks_a_good_move_and_a_bad_one() {
    let mut app = App::new(pack());
    endings_mate_in_2(&mut app);
    app.enter();
    type_move(&mut app, "Rc1+");
    app.tick_at(app.move_entry().reply_deadline().unwrap());
    type_move(&mut app, "Kb6");
    let screen = render(&app, 100, 30);
    assert!(screen.contains("Rc1+"), "{screen}");
    assert!(screen.contains("✓"), "{screen}");
    assert!(screen.contains("Kb6"), "{screen}");
    assert!(screen.contains("✗"), "{screen}");
    assert!(screen.contains("Not the best move, try again"), "{screen}");
    assert!(screen.contains("White played Re1"), "{screen}");
    assert!(screen.contains("F1 hint"), "{screen}");
}
