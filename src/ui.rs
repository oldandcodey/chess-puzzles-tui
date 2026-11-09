//! Rendering. Fixed-width ASCII only (SSH/Tailscale safe, no wide unicode).

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{symbols, Frame};

use crate::app::{fmt_clock, grade_label, App, Phase, Screen, MENU_ITEMS};
use crate::board::{
    border_line, empty_glyph, file_label, format_solution, parse_board, piece_lines, rank_caps,
    row_cells, PieceLine, BOARD_WIDTH,
};
use crate::moves::{Feedback, MoveMark};
use crate::pack::Puzzle;
use crate::progress::Totals;

const GOLD: Color = Color::Rgb(240, 200, 80);
const CYAN: Color = Color::Rgb(100, 190, 230);
const GREEN: Color = Color::Rgb(110, 210, 120);
const RED: Color = Color::Rgb(230, 90, 90);
const AMBER: Color = Color::Rgb(230, 160, 70);
const MUTED: Color = Color::Rgb(130, 135, 145);
const TEXT: Color = Color::Rgb(215, 215, 215);
const WHITE_PIECE: Color = Color::Rgb(245, 240, 220);
const BLACK_PIECE: Color = Color::Rgb(120, 210, 235);
const LIGHT_SQ: Color = Color::Rgb(78, 84, 96);
const DARK_SQ: Color = Color::Rgb(38, 42, 50);

const BANNER: [&str; 5] = [
    "+==========================================+",
    "|                                          |",
    "|     C H E S S    P U Z Z L E S           |",
    "|     mate-in-N  *  set up a real board    |",
    "+==========================================+",
];

/// Plain ASCII border set (no box-drawing glyphs).
const ASCII: symbols::border::Set = symbols::border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

fn block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(ASCII)
        .border_style(Style::default().fg(MUTED))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        ))
}

fn status_line(app: &App) -> Line<'static> {
    let p = &app.pack;
    Line::from(vec![
        Span::styled(" Pack: ", Style::default().fg(MUTED)),
        Span::styled(
            format!(
                "M1 {}  M2 {}  M3 {}  ({} total)",
                p.count(1),
                p.count(2),
                p.count(3),
                p.puzzles.len()
            ),
            Style::default().fg(GREEN),
        ),
        Span::styled("   Lichess CC0", Style::default().fg(MUTED)),
    ])
}

pub fn draw(f: &mut Frame, app: &App) {
    let [body, status, footer] = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let celebrating = app.showing_celebration();
    match app.screen {
        Screen::Menu => draw_menu(f, app, body),
        Screen::Endings if celebrating => draw_celebration(f, app, body),
        Screen::Endings => draw_endings(f, app, body),
        Screen::Progress => draw_progress(f, app, body),
    }
    f.render_widget(Paragraph::new(status_line(app)), status);
    let keys = match app.screen {
        Screen::Menu => " Up/Down Enter  E Endings  P Progress  Q/Esc Quit",
        Screen::Endings if celebrating => " Tab pieces  any other key skips",
        Screen::Endings => match app.phase {
            Phase::Setup => {
                " 1/2/3 level  N/B puzzle  +/- time  Enter start  Tab pieces  Q/Esc menu"
            }
            Phase::Solving => " Enter check  Bksp edit  F1 hint  F2 reveal  Esc setup  Tab pieces",
            Phase::Review => " Y solved  2 second try  H hint  N missed  Tab pieces  Q/Esc setup",
        },
        Screen::Progress => " Q/Esc back",
    };
    f.render_widget(
        Paragraph::new(Span::styled(keys, Style::default().fg(CYAN))),
        footer,
    );
}

fn draw_menu(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = BANNER
        .iter()
        .map(|s| Line::from(Span::styled(format!("  {s}"), Style::default().fg(GOLD))))
        .collect();
    lines.push(Line::from(""));
    for (i, (key, label)) in MENU_ITEMS.iter().enumerate() {
        let sel = i == app.menu_sel;
        let marker = if sel { " > " } else { "   " };
        let st = if sel {
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(GOLD)),
            Span::styled(format!("[{key}] "), Style::default().fg(CYAN)),
            Span::styled(*label, st),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "   Set the puzzle up on a real board, solve it, then check yourself.",
        Style::default().fg(MUTED),
    )));
    f.render_widget(Paragraph::new(lines).block(block("Chess Puzzles")), area);
}

const BOX_INNER: usize = 42;

fn draw_celebration(f: &mut Frame, app: &App, area: Rect) {
    let Some(info) = app.celebration() else {
        return;
    };
    let mate_in = info.mate_in;
    let points = info.points;
    let puzzle_id = info.puzzle_id.clone();
    let gold = Style::default().fg(GOLD);
    let green = Style::default().fg(GREEN).add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(MUTED);
    let mut lines = vec![Line::from(Span::styled(box_edge(), gold))];
    push_boxed(&mut lines, "*    *    *    *    *", gold);
    for star in ["*", "***", "*****"] {
        push_boxed(&mut lines, star, gold);
    }
    push_boxed(&mut lines, "* CLEAN SOLVE *", green);
    for star in ["*****", "***", "*"] {
        push_boxed(&mut lines, star, gold);
    }
    push_boxed(&mut lines, "*    *    *    *    *", gold);
    push_boxed(&mut lines, "", gold);
    push_boxed(
        &mut lines,
        &format!("mate in {mate_in}   +{points}"),
        gold.add_modifier(Modifier::BOLD),
    );
    push_boxed(&mut lines, &puzzle_id, muted);
    push_boxed(&mut lines, "", gold);
    push_boxed(&mut lines, "any other key skips", muted);
    lines.push(Line::from(Span::styled(box_edge(), gold)));
    f.render_widget(Paragraph::new(lines).block(block("Clean solve")), area);
}

fn box_edge() -> String {
    format!("  +{}+", "=".repeat(BOX_INNER))
}

fn push_boxed(lines: &mut Vec<Line<'static>>, text: &str, style: Style) {
    let text: String = text.chars().take(BOX_INNER).collect();
    let pad = BOX_INNER - text.chars().count();
    let left = pad / 2;
    let right = pad - left;
    let edge = Style::default().fg(GOLD);
    lines.push(Line::from(vec![
        Span::styled("  |", edge),
        Span::raw(" ".repeat(left)),
        Span::styled(text, style),
        Span::raw(" ".repeat(right)),
        Span::styled("|", edge),
    ]));
}

fn draw_endings(f: &mut Frame, app: &App, area: Rect) {
    let puzzle = app.current().cloned();
    let fen = shown_fen(app, puzzle.as_ref());
    let mut lines = vec![
        meta_line(app, puzzle.as_ref()),
        clock_line(app, &fen, puzzle.as_ref()),
    ];
    match parse_board(&fen) {
        Ok(grid) => {
            let show_list = app.pieces_shown();
            let pieces = if show_list {
                piece_lines(&grid)
            } else {
                Vec::new()
            };
            let beside = (app.phase != Phase::Review && !show_list).then(|| entry_column(app));
            lines.extend(board_lines(&grid, &pieces, beside.as_deref()));
            if app.phase != Phase::Review && show_list {
                lines.push(Line::from(""));
                lines.extend(entry_lines(app));
            }
        }
        Err(_) => lines.push(Line::from(Span::styled(
            "  No puzzle at this level.",
            Style::default().fg(TEXT),
        ))),
    }
    if app.phase == Phase::Setup {
        lines.push(Line::from(Span::styled(
            "  a1 lower left.",
            Style::default().fg(MUTED),
        )));
    }
    if app.phase == Phase::Review {
        lines.push(instruction_line(app, puzzle.as_ref()));
        if let Some(blurb) = puzzle.as_ref().and_then(|p| theme_blurb(&p.themes)) {
            lines.push(Line::from(Span::styled(
                format!("  {blurb}"),
                Style::default().fg(MUTED),
            )));
        }
    }
    lines.push(session_line(app));
    f.render_widget(Paragraph::new(lines).block(block("Endings")), area);
}

fn shown_fen(app: &App, puzzle: Option<&Puzzle>) -> String {
    if app.phase == Phase::Review {
        puzzle.map(|p| p.puzzle_fen.clone()).unwrap_or_default()
    } else {
        app.move_entry().fen()
    }
}

fn meta_line(app: &App, puzzle: Option<&Puzzle>) -> Line<'static> {
    let rating = puzzle
        .map(|p| p.rating.to_string())
        .unwrap_or_else(|| "-".into());
    let id = puzzle.map(|p| p.id.clone()).unwrap_or_else(|| "-".into());
    Line::from(vec![
        Span::styled(
            format!("  Mate in {}   ", app.level),
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        ),
        Span::styled(app.position_label(), Style::default().fg(TEXT)),
        Span::styled(format!("    {rating}"), Style::default().fg(TEXT)),
        Span::styled(format!("    {id}"), Style::default().fg(MUTED)),
    ])
}

fn clock_line(app: &App, fen: &str, puzzle: Option<&Puzzle>) -> Line<'static> {
    let active = fen.split_whitespace().nth(1).or_else(|| {
        puzzle.map(|p| match p.side_to_move.as_str() {
            "white" => "w",
            "black" => "b",
            _ => "",
        })
    });
    let (side, side_color) = match active {
        Some("w") => ("WHITE TO MOVE".to_string(), GOLD),
        Some("b") => ("BLACK TO MOVE".to_string(), CYAN),
        Some(other) if !other.is_empty() => (other.to_string(), TEXT),
        _ => ("NO PUZZLE".to_string(), MUTED),
    };
    let (clock, clock_color) = clock_text(app);
    Line::from(vec![
        Span::styled(
            format!("  {side:<16}"),
            Style::default().fg(side_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {clock}"),
            Style::default()
                .fg(clock_color)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn clock_text(app: &App) -> (String, Color) {
    let limit = app.timer_secs;
    let elapsed = app.elapsed_secs();
    match app.phase {
        Phase::Setup => (format!("timer {}", fmt_clock(limit)), TEXT),
        Phase::Solving if elapsed < limit => {
            let left = limit - elapsed;
            let color = if left <= 10 { AMBER } else { GREEN };
            (format!("{} left", fmt_clock(left)), color)
        }
        Phase::Solving => (format!("over +{}", fmt_clock(elapsed - limit)), RED),
        Phase::Review if elapsed <= limit => (format!("used {}", fmt_clock(elapsed)), GREEN),
        Phase::Review => (format!("used {} (over)", fmt_clock(elapsed)), RED),
    }
}

fn board_lines(
    grid: &[[Option<char>; 8]; 8],
    pieces: &[PieceLine],
    side: Option<&[Vec<Span<'static>>]>,
) -> Vec<Line<'static>> {
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    rows.push(vec![Span::styled(format!("  {}", file_label()), muted())]);
    rows.push(vec![Span::styled(format!("  {}", border_line()), muted())]);
    for row in 0..8 {
        let rank = (8 - row) as u8;
        let (left, right) = rank_caps(rank);
        let mut spans = vec![Span::styled(format!("  {left}"), muted())];
        for cell in row_cells(grid, row) {
            spans.push(cell_span(cell.piece, cell.light));
        }
        spans.push(Span::styled(right, muted()));
        rows.push(spans);
    }
    rows.push(vec![Span::styled(format!("  {}", border_line()), muted())]);
    rows.push(vec![Span::styled(format!("  {}", file_label()), muted())]);

    let mut lines = Vec::with_capacity(rows.len() + pieces.len());
    for (i, mut spans) in rows.into_iter().enumerate() {
        let used: usize = spans.iter().map(Span::width).sum();
        // The two-space indent is already inside the spans. Pad to the
        // indented board width so the piece column lines up.
        let target = 2 + BOARD_WIDTH;
        if used < target {
            spans.push(Span::raw(" ".repeat(target - used)));
        }
        if let Some(piece) = pieces.get(i) {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(piece.text.clone(), piece_style(piece)));
        } else if let Some(extra) = side.and_then(|rows| rows.get(i)) {
            spans.push(Span::raw("  "));
            spans.extend(extra.clone());
        }
        lines.push(Line::from(spans));
    }
    for piece in pieces.iter().skip(lines.len()) {
        lines.push(Line::from(Span::styled(
            format!("  {}", piece.text),
            piece_style(piece),
        )));
    }
    lines
}

fn cell_span(piece: Option<char>, light: bool) -> Span<'static> {
    let bg = if light { LIGHT_SQ } else { DARK_SQ };
    match piece {
        Some(ch) if ch.is_ascii_uppercase() => Span::styled(
            format!(" {ch} "),
            Style::default()
                .fg(WHITE_PIECE)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Some(ch) => Span::styled(
            format!(" {ch} "),
            Style::default()
                .fg(BLACK_PIECE)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        None => Span::styled(empty_glyph(light), Style::default().fg(MUTED).bg(bg)),
    }
}

fn piece_style(line: &PieceLine) -> Style {
    if line.text.starts_with('W') || (line.white && line.text.starts_with(' ')) {
        Style::default().fg(GOLD)
    } else if line.text.starts_with('B') || (!line.white && line.text.starts_with(' ')) {
        Style::default().fg(CYAN)
    } else {
        Style::default().fg(TEXT)
    }
}

const FIELD_BG: Color = Color::Rgb(30, 34, 42);
const FIELD_INNER: usize = 12;

struct EntryCopy {
    log: Vec<Span<'static>>,
    field: Vec<Span<'static>>,
    message: Vec<Span<'static>>,
    extra: Vec<Span<'static>>,
}

fn entry_copy(app: &App) -> EntryCopy {
    let entry = app.move_entry();
    let log = if entry.log().is_empty() {
        vec![Span::styled("(none yet)", Style::default().fg(MUTED))]
    } else {
        let mut spans = Vec::new();
        for (i, mv) in entry.log().iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("   "));
            }
            let (color, mark) = match mv.mark {
                MoveMark::Correct => (GREEN, "✓"),
                MoveMark::Wrong | MoveMark::Illegal => (RED, "✗"),
            };
            spans.push(Span::styled(
                format!("{} {mark}", mv.text),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
        spans
    };

    let message = match entry.feedback() {
        Feedback::Illegal => vec![Span::styled(
            "Illegal move",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        )],
        Feedback::Wrong => vec![Span::styled(
            "Not the best move, try again",
            Style::default().fg(RED),
        )],
        Feedback::None if app.phase == Phase::Setup => vec![Span::styled(
            "Type a move, then Enter. Enter starts the clock.",
            Style::default().fg(MUTED),
        )],
        Feedback::None if app.phase == Phase::Solving && app.elapsed_secs() >= app.timer_secs => {
            vec![Span::styled(
                "Time is up. F2 reveals the line when you want to grade it.",
                Style::default().fg(TEXT),
            )]
        }
        Feedback::None if app.phase == Phase::Solving => vec![Span::styled(
            "Type a move and press Enter.",
            Style::default().fg(MUTED),
        )],
        Feedback::None => Vec::new(),
    };

    let mut extra = Vec::new();
    if let Some(reply) = entry.reply() {
        extra.push(Span::styled(
            format!("{} played {}", reply.side, reply.san),
            Style::default().fg(MUTED),
        ));
    }
    if app.hint_used && app.phase == Phase::Solving {
        let hint = app
            .current()
            .and_then(|p| p.solution_san.first().cloned())
            .unwrap_or_else(|| "?".into());
        let hint_span = Span::styled(
            format!("Hint: {hint}  (the rest stays hidden)"),
            Style::default().fg(TEXT),
        );
        if extra.is_empty() {
            extra.push(hint_span);
        } else {
            extra.push(Span::raw("   "));
            extra.push(hint_span);
        }
    }

    EntryCopy {
        log,
        field: field_spans(entry.buffer(), entry.rejected()),
        message,
        extra,
    }
}

fn field_spans(buffer: &str, rejected: bool) -> Vec<Span<'static>> {
    let bg = Style::default().bg(FIELD_BG);
    let text_color = if rejected { RED } else { TEXT };
    let mut spans = vec![Span::styled("[", bg.fg(MUTED))];
    if !buffer.is_empty() {
        spans.push(Span::styled(
            buffer.to_string(),
            bg.fg(text_color).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(" ", Style::default().bg(GOLD)));
    let used = buffer.chars().count() + 1;
    let pad = FIELD_INNER.saturating_sub(used);
    if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad), bg));
    }
    spans.push(Span::styled("]", bg.fg(MUTED)));
    spans
}

fn entry_column(app: &App) -> Vec<Vec<Span<'static>>> {
    let copy = entry_copy(app);
    let mut rows = vec![Vec::new(); 12];
    rows[0] = vec![Span::styled(
        "Solution",
        Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
    )];
    rows[2] = copy.log;
    rows[4] = vec![Span::styled(
        "Move",
        Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
    )];
    rows[5] = copy.field;
    rows[7] = copy.message;
    rows[8] = copy.extra;
    rows
}

fn entry_lines(app: &App) -> Vec<Line<'static>> {
    let copy = entry_copy(app);
    let mut lines = Vec::new();
    let mut solution = vec![Span::styled(
        "  Solution  ",
        Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
    )];
    solution.extend(copy.log);
    lines.push(Line::from(solution));
    let mut field = vec![Span::styled(
        "  Move  ",
        Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
    )];
    field.extend(copy.field);
    lines.push(Line::from(field));
    if !copy.message.is_empty() {
        let mut message = vec![Span::raw("  ")];
        message.extend(copy.message);
        lines.push(Line::from(message));
    }
    if !copy.extra.is_empty() {
        let mut extra = vec![Span::raw("  ")];
        extra.extend(copy.extra);
        lines.push(Line::from(extra));
    }
    lines
}

fn instruction_line(app: &App, puzzle: Option<&Puzzle>) -> Line<'static> {
    let elapsed = app.elapsed_secs();
    let text = match app.phase {
        Phase::Setup => {
            "White at the bottom (a1 lower left). Set the pieces, then Enter.".to_string()
        }
        Phase::Solving if app.hint_used => {
            let mv = puzzle
                .and_then(|p| p.solution_san.first())
                .map(String::as_str)
                .unwrap_or("?");
            format!("Hint: {mv}  (the rest stays hidden)")
        }
        Phase::Solving if elapsed >= app.timer_secs => {
            "Time is up. F2 reveals the line when you want to grade it.".to_string()
        }
        Phase::Solving => "Type a move and press Enter.".to_string(),
        Phase::Review => {
            let sol = puzzle
                .map(|p| format_solution(&p.solution_san))
                .unwrap_or_default();
            if app.hint_used {
                format!("Hint used.  Solution: {sol}")
            } else {
                format!("Solution: {sol}")
            }
        }
    };
    let color = if app.phase == Phase::Review {
        GOLD
    } else {
        TEXT
    };
    Line::from(Span::styled(
        format!("  {text}"),
        Style::default().fg(color),
    ))
}

fn session_line(app: &App) -> Line<'static> {
    let t = app.session_counts();
    let summary = format!(
        "solved {}  |  second {}  |  hint {}  |  missed {}",
        t.solved, t.second, t.hint, t.missed
    );
    let text = match app.attempts.last() {
        Some(a) => format!("Last {} {}   {summary}", a.puzzle_id, grade_label(a.grade)),
        None => format!("Session  {summary}"),
    };
    Line::from(Span::styled(
        format!("  {text}"),
        Style::default().fg(MUTED),
    ))
}

fn theme_blurb(themes: &[String]) -> Option<String> {
    let kept: Vec<&str> = themes
        .iter()
        .map(String::as_str)
        .filter(|t| !matches!(*t, "mate" | "endgame" | "oneMove") && !t.starts_with("mateIn"))
        .collect();
    if kept.is_empty() {
        None
    } else {
        Some(kept.join(" "))
    }
}

fn draw_progress(f: &mut Frame, app: &App, area: Rect) {
    let history = app.history();
    let mut lines = vec![
        Line::from(Span::styled(
            "  All time (data/progress.json)",
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        )),
        tally_line("All", history.totals()),
    ];
    for level in 1..=3 {
        lines.push(tally_line(
            &format!("Mate in {level}"),
            history.totals_for(level),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  This session",
        Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
    )));
    lines.push(tally_line("All", session_totals(app, None)));
    for level in 1..=3 {
        lines.push(tally_line(
            &format!("Mate in {level}"),
            session_totals(app, Some(level)),
        ));
    }
    if let Some(note) = progress_note(app) {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {note}"),
            Style::default().fg(AMBER),
        )));
    }
    f.render_widget(Paragraph::new(lines).block(block("Progress")), area);
}

fn session_totals(app: &App, level: Option<u8>) -> Totals {
    let counts = match level {
        Some(mate_in) => app.session_counts_for(mate_in),
        None => app.session_counts(),
    };
    let points = app
        .attempts
        .iter()
        .filter(|attempt| level.is_none_or(|mate_in| attempt.mate_in == mate_in))
        .map(|attempt| attempt.points)
        .sum();
    Totals {
        solved: counts.solved,
        second: counts.second,
        hint: counts.hint,
        missed: counts.missed,
        attempts: counts.solved + counts.second + counts.hint + counts.missed,
        points,
    }
}

fn progress_note(app: &App) -> Option<String> {
    let load = app.history().warning();
    let save = app.save_warning.as_deref();
    match (load, save) {
        (Some(load), Some(save)) if load != save => Some(format!("{load}; {save}")),
        (Some(text), _) | (_, Some(text)) => Some(text.to_string()),
        (None, None) => None,
    }
}

fn tally_line(label: &str, t: Totals) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label:<9}"), Style::default().fg(MUTED)),
        Span::styled(
            format!("solved {:<3} ", t.solved),
            Style::default().fg(GREEN),
        ),
        Span::styled(
            format!("second {:<3} ", t.second),
            Style::default().fg(GOLD),
        ),
        Span::styled(format!("hint {:<3} ", t.hint), Style::default().fg(CYAN)),
        Span::styled(
            format!("missed {:<3} ", t.missed),
            Style::default().fg(TEXT),
        ),
        Span::styled(
            format!("attempts {:<3} ", t.attempts),
            Style::default().fg(TEXT),
        ),
        Span::styled(format!("points {}", t.points), Style::default().fg(GOLD)),
    ])
}

fn muted() -> Style {
    Style::default().fg(MUTED)
}
