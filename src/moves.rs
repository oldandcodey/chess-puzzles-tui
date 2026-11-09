//! Typed solution entry.
//!
//! The box is a buffer plus a [`Line`]: the puzzle position, how far the
//! solver has got, and the green/red moves already submitted. Parsing accepts
//! SAN and coordinate notation, and is case-tolerant when only one legal
//! move matches.

use std::time::{Duration, Instant};

use shakmaty::fen::Fen;
use shakmaty::san::{San, SanPlus};
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Move, Position};

/// How long the solver's move stays on the board before the scripted reply.
pub const REPLY_PAUSE: Duration = Duration::from_millis(600);

const FIELD_LIMIT: usize = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveMark {
    Correct,
    Wrong,
    Illegal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedMove {
    pub text: String,
    pub mark: MoveMark,
}

/// What the panel says under the move box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feedback {
    None,
    /// Legal, but not the puzzle move.
    Wrong,
    /// Not a legal move in this position.
    Illegal,
}

/// The opponent's last scripted reply, kept visible beside later attempts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedReply {
    pub side: &'static str,
    pub san: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    Ignored,
    Illegal,
    Wrong,
    Correct,
    Solved,
}

#[derive(Debug, Clone)]
struct PendingReply {
    at: Instant,
    mv: Move,
    san: String,
    side: &'static str,
}

/// The move box and the line being solved.
#[derive(Debug, Clone)]
pub struct MoveEntry {
    buffer: String,
    /// The text in the box is a move the puzzle rejected.
    rejected: bool,
    pos: Chess,
    solution_uci: Vec<String>,
    solution_san: Vec<String>,
    /// Index of the next move in `solution_uci`. Even indexes are the solver.
    ply: usize,
    log: Vec<LoggedMove>,
    feedback: Feedback,
    reply: Option<PlayedReply>,
    pending: Option<PendingReply>,
    had_wrong: bool,
}

impl MoveEntry {
    pub fn from_puzzle(fen: &str, solution_uci: &[String], solution_san: &[String]) -> Self {
        Self {
            buffer: String::new(),
            rejected: false,
            pos: position(fen).unwrap_or_default(),
            solution_uci: solution_uci.to_vec(),
            solution_san: solution_san.to_vec(),
            ply: 0,
            log: Vec::new(),
            feedback: Feedback::None,
            reply: None,
            pending: None,
            had_wrong: false,
        }
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn rejected(&self) -> bool {
        self.rejected && !self.buffer.is_empty()
    }

    pub fn log(&self) -> &[LoggedMove] {
        &self.log
    }

    pub fn feedback(&self) -> &Feedback {
        &self.feedback
    }

    pub fn reply(&self) -> Option<&PlayedReply> {
        self.reply.as_ref()
    }

    pub fn fen(&self) -> String {
        Fen::from_position(self.pos.clone(), EnPassantMode::Legal).to_string()
    }

    pub fn side(&self) -> &'static str {
        match self.pos.turn() {
            Color::White => "white",
            Color::Black => "black",
        }
    }

    pub fn had_wrong(&self) -> bool {
        self.had_wrong
    }

    pub fn waiting_for_reply(&self) -> bool {
        self.pending.is_some()
    }

    pub fn reply_deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|pending| pending.at)
    }

    pub fn push(&mut self, ch: char) {
        if self.pending.is_some() || self.buffer.chars().count() >= FIELD_LIMIT {
            return;
        }
        self.rejected = false;
        self.buffer.push(ch);
    }

    pub fn backspace(&mut self) {
        if self.pending.is_some() {
            return;
        }
        self.rejected = false;
        self.buffer.pop();
    }

    pub fn submit(&mut self, now: Instant) -> SubmitOutcome {
        if self.pending.is_some() {
            return SubmitOutcome::Ignored;
        }
        let text = self.buffer.trim();
        if text.is_empty() {
            return SubmitOutcome::Ignored;
        }
        let Some(mv) = interpret(&self.pos, text) else {
            let shown = text.to_string();
            self.buffer = shown.clone();
            self.rejected = true;
            self.feedback = Feedback::Illegal;
            self.log.push(LoggedMove {
                text: shown,
                mark: MoveMark::Illegal,
            });
            return SubmitOutcome::Illegal;
        };

        let uci = uci_of(&mv);
        let on_final = self.ply + 1 == self.solution_uci.len();
        let scripted = self
            .solution_uci
            .get(self.ply)
            .is_some_and(|expected| expected == &uci);
        let mates = on_final && mates_after(&self.pos, &mv);
        if scripted || mates {
            let shown = if scripted {
                self.solution_san
                    .get(self.ply)
                    .cloned()
                    .filter(|san| !san.is_empty())
                    .unwrap_or_else(|| san_of(&self.pos, &mv))
            } else {
                san_of(&self.pos, &mv)
            };
            self.log.push(LoggedMove {
                text: shown,
                mark: MoveMark::Correct,
            });
            self.buffer.clear();
            self.rejected = false;
            self.feedback = Feedback::None;
            self.reply = None;
            self.pos.play_unchecked(&mv);
            self.ply += 1;
            if on_final {
                SubmitOutcome::Solved
            } else {
                self.schedule_reply(now);
                SubmitOutcome::Correct
            }
        } else {
            let shown = san_of(&self.pos, &mv);
            self.buffer = shown.clone();
            self.rejected = true;
            self.had_wrong = true;
            self.feedback = Feedback::Wrong;
            self.log.push(LoggedMove {
                text: shown,
                mark: MoveMark::Wrong,
            });
            SubmitOutcome::Wrong
        }
    }

    /// Play the scripted reply once `now` has reached its deadline.
    pub fn tick(&mut self, now: Instant) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| now >= pending.at)
        {
            self.play_reply();
        }
    }

    fn schedule_reply(&mut self, now: Instant) {
        let Some(uci) = self.solution_uci.get(self.ply).cloned() else {
            return;
        };
        let Ok(parsed) = UciMove::from_ascii(uci.as_bytes()) else {
            return;
        };
        let Ok(mv) = parsed.to_move(&self.pos) else {
            return;
        };
        let san = self
            .solution_san
            .get(self.ply)
            .cloned()
            .filter(|san| !san.is_empty())
            .unwrap_or_else(|| san_of(&self.pos, &mv));
        let side = match self.pos.turn() {
            Color::White => "White",
            Color::Black => "Black",
        };
        self.pending = Some(PendingReply {
            at: now + REPLY_PAUSE,
            mv,
            san,
            side,
        });
    }

    fn play_reply(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.pos.play_unchecked(&pending.mv);
        self.ply += 1;
        self.reply = Some(PlayedReply {
            side: pending.side,
            san: pending.san,
        });
    }
}

fn mates_after(pos: &Chess, mv: &Move) -> bool {
    let mut next = pos.clone();
    next.play_unchecked(mv);
    next.is_checkmate()
}

fn san_of(pos: &Chess, mv: &Move) -> String {
    SanPlus::from_move(pos.clone(), mv).to_string()
}

fn uci_of(mv: &Move) -> String {
    UciMove::from_standard(mv).to_string()
}

/// The one legal move `input` names, if the text is unambiguous.
pub fn interpret(pos: &Chess, input: &str) -> Option<Move> {
    let text = input.trim();
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return None;
    }
    if let Some(mv) = try_parse(pos, text) {
        return Some(mv);
    }
    let mut found: Vec<Move> = Vec::new();
    for alt in case_folds(text) {
        if alt == text {
            continue;
        }
        if let Some(mv) = try_parse(pos, &alt) {
            let uci = uci_of(&mv);
            if found.iter().all(|have| uci_of(have) != uci) {
                found.push(mv);
            }
        }
    }
    if found.len() == 1 {
        found.pop()
    } else {
        None
    }
}

fn try_parse(pos: &Chess, text: &str) -> Option<Move> {
    if let Ok(san) = San::from_ascii(text.as_bytes()) {
        if let Ok(mv) = san.to_move(pos) {
            return Some(mv);
        }
    }
    if let Ok(uci) = UciMove::from_ascii(text.as_bytes()) {
        if let Ok(mv) = uci.to_move(pos) {
            return Some(mv);
        }
    }
    None
}

fn case_folds(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = text.to_ascii_lowercase();
    let suffix = if lower.ends_with('#') {
        "#"
    } else if lower.ends_with('+') {
        "+"
    } else {
        ""
    };
    let body = lower.trim_end_matches(['+', '#']);
    if let Some(castle) = match body {
        "o-o" | "0-0" => Some("O-O"),
        "o-o-o" | "0-0-0" => Some("O-O-O"),
        _ => None,
    } {
        out.push(format!("{castle}{suffix}"));
    }
    out.push(lower.clone());
    if let Some(piece) = piece_fold(&lower) {
        out.push(piece);
    }
    out
}

/// `nf3` becomes `Nf3`. A leading pawn file is left alone.
fn piece_fold(lower: &str) -> Option<String> {
    let mut chars = lower.chars();
    let first = chars.next()?;
    if !matches!(first, 'n' | 'b' | 'r' | 'q' | 'k') {
        return None;
    }
    let mut folded = String::new();
    folded.push(first.to_ascii_uppercase());
    folded.push_str(&upper_promotion(&lower[first.len_utf8()..]));
    Some(folded)
}

fn upper_promotion(text: &str) -> String {
    let mut promo = false;
    let mut out = String::new();
    for ch in text.chars() {
        if promo {
            out.push(ch.to_ascii_uppercase());
            promo = false;
        } else {
            if ch == '=' {
                promo = true;
            }
            out.push(ch);
        }
    }
    out
}

fn position(fen: &str) -> Option<Chess> {
    Fen::from_ascii(fen.as_bytes())
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(fen: &str) -> Chess {
        position(fen).expect(fen)
    }

    fn uci(fen: &str, input: &str) -> String {
        uci_of(&interpret(&pos(fen), input).unwrap_or_else(|| panic!("{input} in {fen}")))
    }

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    const CASTLE: &str = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
    const PROMOTE: &str = "k7/4P3/8/8/8/8/8/4K3 w - - 0 1";
    const CAPTURE: &str = "4k3/4p3/8/8/7Q/8/8/4K3 w - - 0 1";

    #[test]
    fn parses_san_and_coordinates_ignoring_harmless_case() {
        assert_eq!(uci(START, "Nf3"), "g1f3");
        assert_eq!(uci(START, "nf3"), "g1f3");
        assert_eq!(uci(START, "e2e4"), "e2e4");
        assert_eq!(uci(START, "E2E4"), "e2e4");
        assert_eq!(uci(CASTLE, "O-O"), "e1g1");
        assert_eq!(uci(CASTLE, "o-o"), "e1g1");
        assert_eq!(uci(CASTLE, "0-0"), "e1g1");
        assert_eq!(uci(PROMOTE, "e8=Q"), "e7e8q");
        assert_eq!(uci(PROMOTE, "e8=q"), "e7e8q");
        assert_eq!(uci(PROMOTE, "e7e8q"), "e7e8q");
        assert_eq!(uci(CAPTURE, "Qxe7+"), "h4e7");
        assert_eq!(uci(CAPTURE, "qxe7+"), "h4e7");
    }

    #[test]
    fn an_illegal_string_is_not_a_move() {
        assert!(interpret(&pos(START), "Qxe7+").is_none());
        assert!(interpret(&pos(START), "e2e5").is_none());
        assert!(interpret(&pos(START), "O-O").is_none());
        assert!(interpret(&pos(START), "").is_none());
        assert!(interpret(&pos(START), "not a move").is_none());
    }

    fn mate_line() -> MoveEntry {
        // 02Ofv: black mates in 2 with Rc1+, Re1, Rxe1#.
        MoveEntry::from_puzzle(
            "8/p3R3/k4p2/3b2p1/7P/7P/PPr2P1B/6KR b - - 0 33",
            &uci_list(&["c2c1", "e7e1", "c1e1"]),
            &san_list(&["Rc1+", "Re1", "Rxe1#"]),
        )
    }

    fn uci_list(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| (*s).to_string()).collect()
    }

    fn san_list(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_wrong_move_is_kept_off_the_board_and_an_illegal_one_too() {
        let mut line = mate_line();
        let fen = line.fen();
        line.buffer = "Nf3".into();
        assert_eq!(line.submit(Instant::now()), SubmitOutcome::Illegal);
        assert_eq!(line.log()[0].mark, MoveMark::Illegal);
        assert_eq!(line.feedback(), &Feedback::Illegal);
        assert_eq!(line.fen(), fen);
        assert!(!line.had_wrong());

        line.buffer = "Kb6".into();
        assert_eq!(line.submit(Instant::now()), SubmitOutcome::Wrong);
        assert_eq!(line.log()[1].text, "Kb6");
        assert_eq!(line.log()[1].mark, MoveMark::Wrong);
        assert_eq!(line.feedback(), &Feedback::Wrong);
        assert_eq!(line.fen(), fen);
        assert!(line.had_wrong());
    }

    #[test]
    fn a_correct_move_plays_then_the_reply_arrives_after_the_pause() {
        let mut line = mate_line();
        let now = Instant::now();
        line.buffer = "Rc1+".into();
        assert_eq!(line.submit(now), SubmitOutcome::Correct);
        assert_eq!(line.log()[0].text, "Rc1+");
        assert_eq!(line.log()[0].mark, MoveMark::Correct);
        assert!(line
            .fen()
            .starts_with("8/p3R3/k4p2/3b2p1/7P/7P/PP3P1B/2r3KR"));
        assert!(line.waiting_for_reply());
        assert!(line.reply().is_none());

        line.push('K');
        assert_eq!(line.buffer(), "", "no typing during the reply pause");

        let deadline = line.reply_deadline().unwrap();
        line.tick(deadline - Duration::from_millis(1));
        assert!(line.waiting_for_reply());
        line.tick(deadline);
        let reply = line.reply().unwrap();
        assert_eq!(reply.side, "White");
        assert_eq!(reply.san, "Re1");
        assert!(line
            .fen()
            .starts_with("8/p7/k4p2/3b2p1/7P/7P/PP3P1B/2r1R1KR"));
        assert_eq!(line.side(), "black");
    }

    #[test]
    fn the_whole_mate_completes_on_the_last_move() {
        let mut line = mate_line();
        let now = Instant::now();
        line.buffer = "rc1+".into();
        assert_eq!(line.submit(now), SubmitOutcome::Correct);
        line.tick(line.reply_deadline().unwrap());
        line.buffer = "c1e1".into();
        assert_eq!(line.submit(now), SubmitOutcome::Solved);
        assert!(line.log().iter().all(|mv| mv.mark == MoveMark::Correct));
        assert_eq!(line.log().len(), 2);
        assert!(!line.had_wrong());
    }

    #[test]
    fn the_final_move_accepts_any_other_mate() {
        // Two rooks both mate. The scripted move is Ra8#. Rb8# is also mate.
        let mut line = MoveEntry::from_puzzle(
            "7k/R7/1R4K1/8/8/8/8/8 w - - 0 1",
            &uci_list(&["a7a8"]),
            &san_list(&["Ra8#"]),
        );
        line.buffer = "Rb8".into();
        assert_eq!(line.submit(Instant::now()), SubmitOutcome::Solved);
        assert_eq!(line.log()[0].mark, MoveMark::Correct);
        assert_eq!(line.log()[0].text, "Rb8#");

        let mut scripted = MoveEntry::from_puzzle(
            "7k/R7/1R4K1/8/8/8/8/8 w - - 0 1",
            &uci_list(&["a7a8"]),
            &san_list(&["Ra8#"]),
        );
        scripted.buffer = "Ra8#".into();
        assert_eq!(scripted.submit(Instant::now()), SubmitOutcome::Solved);

        let mut quiet = MoveEntry::from_puzzle(
            "7k/R7/1R4K1/8/8/8/8/8 w - - 0 1",
            &uci_list(&["a7a8"]),
            &san_list(&["Ra8#"]),
        );
        let before = quiet.fen();
        quiet.buffer = "Kg5".into();
        assert_eq!(quiet.submit(Instant::now()), SubmitOutcome::Wrong);
        assert_eq!(quiet.fen(), before);
    }
}
