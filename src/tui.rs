//! Terminal shell around [`Picker`]: raw mode, key events and rendering. All
//! decisions live in `picker`; this module only draws state and feeds it keys.

use crate::picker::{Flow, Picker, action_for, clamp_scroll};
use anyhow::{Context, Result};
use crossterm::event::{self, Event};
use crossterm::style::{Attribute, Color, ContentStyle, PrintStyledContent, Stylize};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{cursor, queue};
use std::io::{self, Write};

const HELP: &[(&str, &str)] = &[
    ("tab", "select"),
    ("↑/↓", "move"),
    ("enter", "confirm"),
    ("esc", "cancel"),
];

/// Rows the picker draws around the list: prompt, counters and help.
const CHROME_ROWS: u16 = 3;

const PROMPT_SEPARATOR: &str = " ❯ ";
const COLUMN_GAP: usize = 2;

/// A run of text drawn in one style.
#[derive(Debug, Clone, PartialEq)]
struct Segment {
    text: String,
    style: ContentStyle,
}

impl Segment {
    fn new(text: impl Into<String>, style: ContentStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

fn plain() -> ContentStyle {
    ContentStyle::new()
}

fn bold() -> ContentStyle {
    ContentStyle::new().attribute(Attribute::Bold)
}

fn dim() -> ContentStyle {
    ContentStyle::new().with(Color::DarkGrey)
}

fn accent() -> ContentStyle {
    ContentStyle::new()
        .with(Color::Cyan)
        .attribute(Attribute::Bold)
}

/// Style of a list column: the leading column stands out and later ones recede.
fn column_style(column: usize, highlighted: bool) -> ContentStyle {
    match column {
        0 if highlighted => accent(),
        0 => bold(),
        1 => ContentStyle::new().with(Color::Blue),
        2 => ContentStyle::new().with(Color::Magenta),
        _ => dim(),
    }
}

fn match_style(base: ContentStyle) -> ContentStyle {
    base.with(Color::Yellow)
        .attribute(Attribute::Bold)
        .attribute(Attribute::Underlined)
}

/// Show the picker over `rows` of cells and return the indices of the selected
/// rows, or `None` if the user cancelled.
pub fn select(message: &str, rows: Vec<Vec<String>>) -> Result<Option<Vec<usize>>> {
    let mut out = io::stderr();
    enable_raw_mode().context("entering raw mode")?;
    let entered = queue!(out, EnterAlternateScreen).and_then(|()| out.flush());

    let result = entered
        .context("entering alternate screen")
        .and_then(|()| run(&mut out, message, rows));

    let _ = queue!(out, LeaveAlternateScreen, cursor::Show).and_then(|()| out.flush());
    disable_raw_mode().context("leaving raw mode")?;
    result
}

fn run<W: Write>(out: &mut W, message: &str, rows: Vec<Vec<String>>) -> Result<Option<Vec<usize>>> {
    let mut picker = Picker::new(rows);
    let widths = column_widths(&picker);
    let mut scroll = 0;
    loop {
        let (cols, term_rows) = terminal::size().context("reading terminal size")?;
        let height = list_height(term_rows);
        scroll = clamp_scroll(scroll, picker.highlight(), height);
        render(out, message, &picker, &widths, scroll, cols, height)
            .context("drawing the picker")?;

        let Event::Key(key) = event::read().context("reading a key event")? else {
            continue;
        };
        let Some(action) = action_for(key) else {
            continue;
        };
        match picker.apply(action) {
            Flow::Continue => {}
            Flow::Accept => return Ok(Some(picker.selection())),
            Flow::Cancel => return Ok(None),
        }
    }
}

fn list_height(rows: u16) -> usize {
    rows.saturating_sub(CHROME_ROWS).max(1) as usize
}

fn render<W: Write>(
    out: &mut W,
    message: &str,
    picker: &Picker,
    widths: &[usize],
    scroll: usize,
    cols: u16,
    height: usize,
) -> io::Result<()> {
    queue!(
        out,
        cursor::Hide,
        cursor::MoveTo(0, 0),
        Clear(ClearType::FromCursorDown)
    )?;
    print_line(out, &prompt(message, &picker.query()), cols)?;
    queue!(out, cursor::MoveToNextLine(1))?;
    print_line(out, &counters(picker), cols)?;

    let visible = picker
        .matches()
        .iter()
        .enumerate()
        .skip(scroll)
        .take(height);
    for (row, &index) in visible {
        let highlighted = row == picker.highlight();
        queue!(out, cursor::MoveToNextLine(1))?;
        print_line(out, &row_segments(picker, index, highlighted, widths), cols)?;
    }

    queue!(out, cursor::MoveTo(0, CHROME_ROWS - 1 + height as u16))?;
    print_line(out, &help(), cols)?;
    queue!(
        out,
        cursor::MoveTo(caret_column(message, picker.caret(), cols), 0),
        cursor::Show,
    )?;
    out.flush()
}

fn print_line<W: Write>(out: &mut W, segments: &[Segment], cols: u16) -> io::Result<()> {
    for segment in clip(segments, cols) {
        queue!(out, PrintStyledContent(segment.style.apply(segment.text)))?;
    }
    Ok(())
}

fn prompt(message: &str, query: &str) -> Vec<Segment> {
    vec![
        Segment::new(message, bold()),
        Segment::new(PROMPT_SEPARATOR, accent()),
        Segment::new(query, plain()),
    ]
}

fn counters(picker: &Picker) -> Vec<Segment> {
    let selected = picker.selected_count();
    let selected_style = if selected > 0 {
        ContentStyle::new().with(Color::Green)
    } else {
        dim()
    };
    vec![
        Segment::new(
            format!("  {}/{} · ", picker.matches().len(), picker.total()),
            dim(),
        ),
        Segment::new(format!("{selected} selected"), selected_style),
    ]
}

fn help() -> Vec<Segment> {
    let mut segments = vec![Segment::new("  ", plain())];
    for (i, (key, action)) in HELP.iter().enumerate() {
        if i > 0 {
            segments.push(Segment::new(" · ", dim()));
        }
        segments.push(Segment::new(*key, plain()));
        segments.push(Segment::new(format!(" {action}"), dim()));
    }
    segments
}

/// Widest cell of each column across every row, so columns stay put while the
/// match list changes.
fn column_widths(picker: &Picker) -> Vec<usize> {
    let mut widths = Vec::new();
    for index in 0..picker.total() {
        for (column, cell) in picker.cells(index).iter().enumerate() {
            let len = cell.chars().count();
            match widths.get_mut(column) {
                Some(width) => *width = len.max(*width),
                None => widths.push(len),
            }
        }
    }
    widths
}

/// One list row: pointer, selection mark and the aligned, styled cells with the
/// query's matched characters emphasized. Columns empty in every row are skipped.
fn row_segments(
    picker: &Picker,
    index: usize,
    highlighted: bool,
    widths: &[usize],
) -> Vec<Segment> {
    let mut segments = vec![
        if highlighted {
            Segment::new("❯ ", accent())
        } else {
            Segment::new("  ", plain())
        },
        if picker.is_selected(index) {
            Segment::new("● ", ContentStyle::new().with(Color::Green))
        } else {
            Segment::new("○ ", dim())
        },
    ];

    let matched = picker.matched_chars(index);
    let last = widths.iter().rposition(|&w| w > 0);
    let columns = picker.cells(index).iter().zip(&matched).zip(widths);
    let mut first = true;
    for (column, ((cell, hits), &width)) in columns.enumerate() {
        if width == 0 {
            continue;
        }
        if !first {
            segments.push(Segment::new(" ".repeat(COLUMN_GAP), plain()));
        }
        first = false;
        segments.extend(highlight_matches(
            cell,
            hits,
            column_style(column, highlighted),
        ));
        let padding = width - cell.chars().count();
        if padding > 0 && Some(column) != last {
            segments.push(Segment::new(" ".repeat(padding), plain()));
        }
    }
    segments
}

/// Split `text` into runs, drawing the characters at the sorted offsets `hits`
/// in the match style.
fn highlight_matches(text: &str, hits: &[usize], style: ContentStyle) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut hits = hits.iter().peekable();
    for (i, c) in text.chars().enumerate() {
        let run_style = if hits.next_if(|&&h| h == i).is_some() {
            match_style(style)
        } else {
            style
        };
        match segments.last_mut() {
            Some(last) if last.style == run_style => last.text.push(c),
            _ => segments.push(Segment::new(c, run_style)),
        }
    }
    segments
}

/// Cut `segments` so their combined text fits in `cols` characters.
fn clip(segments: &[Segment], cols: u16) -> Vec<Segment> {
    let mut remaining = cols as usize;
    let mut clipped = Vec::new();
    for segment in segments {
        if remaining == 0 {
            break;
        }
        let text: String = segment.text.chars().take(remaining).collect();
        remaining -= text.chars().count();
        clipped.push(Segment::new(text, segment.style));
    }
    clipped
}

/// Column of the query caret on the prompt line, clamped to the last column.
fn caret_column(message: &str, caret: usize, cols: u16) -> u16 {
    let offset = message.chars().count() + PROMPT_SEPARATOR.chars().count() + caret;
    u16::try_from(offset)
        .unwrap_or(u16::MAX)
        .min(cols.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::Action;

    fn text(segments: &[Segment]) -> String {
        segments.iter().map(|s| s.text.as_str()).collect()
    }

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn counters_report_matches_total_and_selection() {
        let mut picker = Picker::new(vec![row(&["alpha"]), row(&["beta"])]);
        assert_eq!(text(&counters(&picker)), "  2/2 · 0 selected");

        picker.apply(Action::ToggleHighlighted);
        picker.apply(Action::Insert('b'));
        assert_eq!(text(&counters(&picker)), "  1/2 · 1 selected");
    }

    #[test]
    fn rows_align_columns_and_skip_columns_empty_everywhere() {
        let mut picker = Picker::new(vec![
            row(&["GitHub", "", "personal"]),
            row(&["SONAR_TOKEN", "", "work"]),
        ]);
        let widths = column_widths(&picker);
        assert_eq!(widths, [11, 0, 8]);
        picker.apply(Action::ToggleHighlighted);
        assert_eq!(
            text(&row_segments(&picker, 0, false, &widths)),
            "  ● GitHub       personal"
        );
        assert_eq!(
            text(&row_segments(&picker, 1, true, &widths)),
            "❯ ○ SONAR_TOKEN  work"
        );
    }

    #[test]
    fn matched_characters_get_the_match_style() {
        let base = plain();
        assert_eq!(
            highlight_matches("abcd", &[1, 2], base),
            [
                Segment::new("a", base),
                Segment::new("bc", match_style(base)),
                Segment::new("d", base),
            ]
        );
    }

    #[test]
    fn list_height_leaves_room_for_chrome() {
        assert_eq!(list_height(24), 21);
        assert_eq!(list_height(4), 1);
        assert_eq!(list_height(1), 1);
    }

    #[test]
    fn clip_cuts_across_segments_at_the_terminal_width() {
        let segments = [Segment::new("ab", plain()), Segment::new("cdæø", dim())];
        assert_eq!(text(&clip(&segments, 5)), "abcdæ");
        assert_eq!(text(&clip(&segments, 10)), "abcdæø");
        assert_eq!(clip(&segments, 2).len(), 1);
    }

    #[test]
    fn caret_column_follows_the_query_and_clamps_to_the_width() {
        assert_eq!(caret_column("Select", 0, 80), 9);
        assert_eq!(caret_column("Select", 4, 80), 13);
        assert_eq!(caret_column("Select", 4, 10), 9);
    }
}
