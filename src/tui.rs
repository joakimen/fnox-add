//! Terminal shell around [`Picker`]: raw mode, key events and rendering. All
//! decisions live in `picker`; this module only draws state and feeds it keys.

use crate::picker::{Flow, Picker, action_for, clamp_scroll};
use anyhow::{Context, Result};
use crossterm::event::{self, Event};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{cursor, queue, style};
use std::io::{self, Write};

const HELP: &str = "tab select · ↑/↓ move · enter confirm · esc cancel";

/// Rows the picker draws around the list: prompt, counters and help.
const CHROME_ROWS: u16 = 3;

/// Show the picker and return the indices of the selected labels, or `None` if
/// the user cancelled.
pub fn select(message: &str, labels: Vec<String>) -> Result<Option<Vec<usize>>> {
    let mut out = io::stderr();
    enable_raw_mode().context("entering raw mode")?;
    let entered = queue!(out, EnterAlternateScreen).and_then(|()| out.flush());

    let result = entered
        .context("entering alternate screen")
        .and_then(|()| run(&mut out, message, labels));

    let _ = queue!(out, LeaveAlternateScreen, cursor::Show).and_then(|()| out.flush());
    disable_raw_mode().context("leaving raw mode")?;
    result
}

fn run<W: Write>(out: &mut W, message: &str, labels: Vec<String>) -> Result<Option<Vec<usize>>> {
    let mut picker = Picker::new(labels);
    let mut scroll = 0;
    loop {
        let (cols, rows) = terminal::size().context("reading terminal size")?;
        let height = list_height(rows);
        scroll = clamp_scroll(scroll, picker.highlight(), height);
        render(out, message, &picker, scroll, cols, height).context("drawing the picker")?;

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
    scroll: usize,
    cols: u16,
    height: usize,
) -> io::Result<()> {
    let query = picker.query();
    queue!(
        out,
        cursor::Hide,
        cursor::MoveTo(0, 0),
        Clear(ClearType::FromCursorDown),
        style::Print(truncate(&format!("{message} {query}"), cols)),
        cursor::MoveToNextLine(1),
        style::SetForegroundColor(style::Color::DarkGrey),
        style::Print(truncate(&counters(picker), cols)),
        style::ResetColor,
    )?;

    let visible = picker
        .matches()
        .iter()
        .enumerate()
        .skip(scroll)
        .take(height);
    for (row, &index) in visible {
        let highlighted = row == picker.highlight();
        let pointer = if highlighted { "❯ " } else { "  " };
        let mark = if picker.is_selected(index) {
            "[x] "
        } else {
            "[ ] "
        };
        let line = truncate(&format!("{pointer}{mark}{}", picker.label(index)), cols);
        queue!(
            out,
            cursor::MoveToNextLine(1),
            style::SetForegroundColor(if highlighted {
                style::Color::Cyan
            } else {
                style::Color::Reset
            }),
            style::Print(line),
            style::ResetColor,
        )?;
    }

    queue!(
        out,
        cursor::MoveTo(0, CHROME_ROWS - 1 + height as u16),
        style::SetForegroundColor(style::Color::DarkGrey),
        style::Print(truncate(HELP, cols)),
        style::ResetColor,
        cursor::MoveTo(caret_column(message, picker.caret(), cols), 0),
        cursor::Show,
    )?;
    out.flush()
}

fn counters(picker: &Picker) -> String {
    format!(
        "  {}/{} · {} selected",
        picker.matches().len(),
        picker.total(),
        picker.selected_count()
    )
}

/// Column of the query caret on the prompt line, clamped to the last column.
fn caret_column(message: &str, caret: usize, cols: u16) -> u16 {
    let offset = message.chars().count() + 1 + caret;
    u16::try_from(offset)
        .unwrap_or(u16::MAX)
        .min(cols.saturating_sub(1))
}

fn truncate(line: &str, cols: u16) -> String {
    line.chars().take(cols as usize).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_height_leaves_room_for_chrome() {
        assert_eq!(list_height(24), 21);
        assert_eq!(list_height(4), 1);
        assert_eq!(list_height(1), 1);
    }

    #[test]
    fn truncate_cuts_at_the_terminal_width() {
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("ærlig", 2), "ær");
    }

    #[test]
    fn caret_column_follows_the_query_and_clamps_to_the_width() {
        assert_eq!(caret_column("Select:", 0, 80), 8);
        assert_eq!(caret_column("Select:", 4, 80), 12);
        assert_eq!(caret_column("Select:", 4, 10), 9);
    }
}
