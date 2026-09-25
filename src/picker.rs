//! Pure picker state: the filter query, the fuzzy match list and the set of
//! selected items. Terminal input is translated into [`Action`]s here and
//! applied without touching the terminal, so the whole interaction is testable.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// A single edit, motion or selection command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Insert(char),
    DeleteCharBack,
    DeleteCharForward,
    DeleteWordBack,
    DeleteToLineStart,
    DeleteToLineEnd,
    MoveCharLeft,
    MoveCharRight,
    MoveLineStart,
    MoveLineEnd,
    HighlightUp,
    HighlightDown,
    ToggleHighlighted,
    Accept,
    Cancel,
}

/// What the caller should do after applying an [`Action`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Accept,
    Cancel,
}

/// Translate a key press into an action, or `None` if the key is unbound.
pub fn action_for(key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let action = match key.code {
        KeyCode::Char('w') if ctrl => Action::DeleteWordBack,
        KeyCode::Char('u') if ctrl => Action::DeleteToLineStart,
        KeyCode::Char('k') if ctrl => Action::DeleteToLineEnd,
        KeyCode::Char('a') if ctrl => Action::MoveLineStart,
        KeyCode::Char('e') if ctrl => Action::MoveLineEnd,
        KeyCode::Char('b') if ctrl => Action::MoveCharLeft,
        KeyCode::Char('f') if ctrl => Action::MoveCharRight,
        KeyCode::Char('p') if ctrl => Action::HighlightUp,
        KeyCode::Char('n') if ctrl => Action::HighlightDown,
        KeyCode::Char('c') if ctrl => Action::Cancel,
        KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
            Action::Insert(c)
        }
        KeyCode::Backspace => Action::DeleteCharBack,
        KeyCode::Delete => Action::DeleteCharForward,
        KeyCode::Left => Action::MoveCharLeft,
        KeyCode::Right => Action::MoveCharRight,
        KeyCode::Home => Action::MoveLineStart,
        KeyCode::End => Action::MoveLineEnd,
        KeyCode::Up => Action::HighlightUp,
        KeyCode::Down => Action::HighlightDown,
        KeyCode::Tab | KeyCode::BackTab => Action::ToggleHighlighted,
        KeyCode::Enter => Action::Accept,
        KeyCode::Esc => Action::Cancel,
        _ => return None,
    };
    Some(action)
}

/// A single-line text input with a caret, both measured in characters.
#[derive(Debug, Default, PartialEq, Eq)]
struct Query {
    text: Vec<char>,
    caret: usize,
}

impl Query {
    fn as_string(&self) -> String {
        self.text.iter().collect()
    }

    fn insert(&mut self, c: char) {
        self.text.insert(self.caret, c);
        self.caret += 1;
    }

    fn delete_char_back(&mut self) {
        if self.caret > 0 {
            self.caret -= 1;
            self.text.remove(self.caret);
        }
    }

    fn delete_char_forward(&mut self) {
        if self.caret < self.text.len() {
            self.text.remove(self.caret);
        }
    }

    /// Delete back over trailing whitespace and then over one whitespace-delimited
    /// word, matching the terminal's own WERASE behavior.
    fn delete_word_back(&mut self) {
        let mut start = self.caret;
        while start > 0 && self.text[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !self.text[start - 1].is_whitespace() {
            start -= 1;
        }
        self.text.drain(start..self.caret);
        self.caret = start;
    }

    fn delete_to_start(&mut self) {
        self.text.drain(..self.caret);
        self.caret = 0;
    }

    fn delete_to_end(&mut self) {
        self.text.truncate(self.caret);
    }

    fn move_left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    fn move_right(&mut self) {
        self.caret = (self.caret + 1).min(self.text.len());
    }
}

/// Filtering and selection state over a fixed list of rows, each a list of
/// cells. The query is matched against a row's cells joined by single spaces.
pub struct Picker {
    rows: Vec<Vec<String>>,
    haystacks: Vec<String>,
    selected: Vec<bool>,
    query: Query,
    /// Indices into `rows`, best match first.
    matches: Vec<usize>,
    /// Sorted character offsets into each row's haystack that the query matched.
    positions: Vec<Vec<u32>>,
    /// Index into `matches`.
    highlight: usize,
    matcher: Matcher,
}

impl Picker {
    pub fn new(rows: Vec<Vec<String>>) -> Self {
        let haystacks = rows.iter().map(|cells| cells.join(" ")).collect();
        let selected = vec![false; rows.len()];
        let matches = (0..rows.len()).collect();
        let positions = vec![Vec::new(); rows.len()];
        Self {
            rows,
            haystacks,
            selected,
            query: Query::default(),
            matches,
            positions,
            highlight: 0,
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    pub fn query(&self) -> String {
        self.query.as_string()
    }

    pub fn caret(&self) -> usize {
        self.query.caret
    }

    /// Indices into the row list, in the order they are displayed.
    pub fn matches(&self) -> &[usize] {
        &self.matches
    }

    pub fn total(&self) -> usize {
        self.rows.len()
    }

    pub fn cells(&self, index: usize) -> &[String] {
        &self.rows[index]
    }

    /// For each cell of a row, the sorted character offsets the query matched.
    pub fn matched_chars(&self, index: usize) -> Vec<Vec<usize>> {
        let mut per_cell = vec![Vec::new(); self.rows[index].len()];
        let mut start = 0;
        let mut positions = self.positions[index].iter().map(|&p| p as usize).peekable();
        for (cell, hits) in self.rows[index].iter().zip(&mut per_cell) {
            let end = start + cell.chars().count();
            while let Some(p) = positions.next_if(|&p| p < end) {
                if p >= start {
                    hits.push(p - start);
                }
            }
            start = end + 1;
        }
        per_cell
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected[index]
    }

    pub fn selected_count(&self) -> usize {
        self.selected.iter().filter(|s| **s).count()
    }

    /// Position of the highlighted row within [`Picker::matches`].
    pub fn highlight(&self) -> usize {
        self.highlight
    }

    /// The selected row indices, in list order.
    pub fn selection(&self) -> Vec<usize> {
        self.selected
            .iter()
            .enumerate()
            .filter(|(_, s)| **s)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn apply(&mut self, action: Action) -> Flow {
        match action {
            Action::Insert(c) => self.edit(|q| q.insert(c)),
            Action::DeleteCharBack => self.edit(Query::delete_char_back),
            Action::DeleteCharForward => self.edit(Query::delete_char_forward),
            Action::DeleteWordBack => self.edit(Query::delete_word_back),
            Action::DeleteToLineStart => self.edit(Query::delete_to_start),
            Action::DeleteToLineEnd => self.edit(Query::delete_to_end),
            Action::MoveCharLeft => self.query.move_left(),
            Action::MoveCharRight => self.query.move_right(),
            Action::MoveLineStart => self.query.caret = 0,
            Action::MoveLineEnd => self.query.caret = self.query.text.len(),
            Action::HighlightUp => self.highlight = self.highlight.saturating_sub(1),
            Action::HighlightDown => self.move_down(),
            Action::ToggleHighlighted => {
                if let Some(&index) = self.matches.get(self.highlight) {
                    self.selected[index] = !self.selected[index];
                    self.move_down();
                }
            }
            Action::Accept => return Flow::Accept,
            Action::Cancel => return Flow::Cancel,
        }
        Flow::Continue
    }

    fn move_down(&mut self) {
        self.highlight = (self.highlight + 1).min(self.matches.len().saturating_sub(1));
    }

    /// Run a query edit and re-filter, dropping the highlight back to the top.
    fn edit(&mut self, f: impl FnOnce(&mut Query)) {
        f(&mut self.query);
        self.filter();
        self.highlight = 0;
    }

    fn filter(&mut self) {
        self.positions.iter_mut().for_each(Vec::clear);
        if self.query.text.is_empty() {
            self.matches = (0..self.rows.len()).collect();
            return;
        }
        let pattern = Pattern::parse(
            &self.query.as_string(),
            CaseMatching::Smart,
            Normalization::Smart,
        );
        let mut buf = Vec::new();
        let mut scored = Vec::new();
        for (i, haystack) in self.haystacks.iter().enumerate() {
            let hits = &mut self.positions[i];
            let Some(score) =
                pattern.indices(Utf32Str::new(haystack, &mut buf), &mut self.matcher, hits)
            else {
                hits.clear();
                continue;
            };
            hits.sort_unstable();
            hits.dedup();
            scored.push((i, score));
        }
        // Stable sort, so equally scored rows keep their catalog order.
        scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        self.matches = scored.into_iter().map(|(i, _)| i).collect();
    }
}

/// Smallest adjustment of `scroll` that keeps row `highlight` inside a viewport
/// of `height` rows.
pub fn clamp_scroll(scroll: usize, highlight: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    if highlight < scroll {
        highlight
    } else if highlight >= scroll + height {
        highlight + 1 - height
    } else {
        scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn ctrl(c: char) -> KeyEvent {
        key(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(ToString::to_string).collect()
    }

    fn picker() -> Picker {
        Picker::new(vec![
            row(&["GH_TOKEN", "personal", "api-keys/github/credential"]),
            row(&["SONAR_TOKEN", "personal", "api-keys/sonarcloud/credential"]),
            row(&["ARTIFACTORY_TOKEN", "work", "work/artifactory/token"]),
        ])
    }

    fn type_query(p: &mut Picker, text: &str) {
        for c in text.chars() {
            p.apply(Action::Insert(c));
        }
    }

    #[test]
    fn tab_toggles_the_highlighted_item() {
        let mut p = picker();
        assert_eq!(
            action_for(key(KeyCode::Tab, KeyModifiers::NONE)),
            Some(Action::ToggleHighlighted)
        );

        p.apply(Action::ToggleHighlighted);
        assert_eq!(p.selection(), vec![0]);
        p.apply(Action::HighlightUp);
        p.apply(Action::ToggleHighlighted);
        assert_eq!(p.selection(), Vec::<usize>::new());
    }

    #[test]
    fn toggling_advances_the_highlight_and_stops_at_the_last_item() {
        let mut p = picker();
        p.apply(Action::ToggleHighlighted);
        assert_eq!(p.highlight(), 1);
        p.apply(Action::ToggleHighlighted);
        p.apply(Action::ToggleHighlighted);
        assert_eq!(p.highlight(), 2);
        assert_eq!(p.selection(), vec![0, 1, 2]);
        p.apply(Action::ToggleHighlighted);
        assert_eq!(p.highlight(), 2);
        assert_eq!(p.selection(), vec![0, 1]);
    }

    #[test]
    fn space_is_typed_into_the_query_rather_than_toggling() {
        let mut p = picker();
        p.apply(action_for(key(KeyCode::Char(' '), KeyModifiers::NONE)).unwrap());
        assert_eq!(p.query(), " ");
        assert!(p.selection().is_empty());
    }

    #[test]
    fn readline_motions_are_bound_to_ctrl_keys() {
        assert_eq!(action_for(ctrl('a')), Some(Action::MoveLineStart));
        assert_eq!(action_for(ctrl('e')), Some(Action::MoveLineEnd));
        assert_eq!(action_for(ctrl('b')), Some(Action::MoveCharLeft));
        assert_eq!(action_for(ctrl('f')), Some(Action::MoveCharRight));
        assert_eq!(action_for(ctrl('w')), Some(Action::DeleteWordBack));
    }

    #[test]
    fn caret_motions_stay_within_the_query() {
        let mut p = picker();
        type_query(&mut p, "abc");
        assert_eq!(p.caret(), 3);

        p.apply(Action::MoveLineStart);
        assert_eq!(p.caret(), 0);
        p.apply(Action::MoveCharLeft);
        assert_eq!(p.caret(), 0);
        p.apply(Action::MoveCharRight);
        assert_eq!(p.caret(), 1);
        p.apply(Action::MoveLineEnd);
        assert_eq!(p.caret(), 3);
        p.apply(Action::MoveCharRight);
        assert_eq!(p.caret(), 3);
    }

    #[test]
    fn inserts_and_deletes_happen_at_the_caret() {
        let mut p = picker();
        type_query(&mut p, "token");
        p.apply(Action::MoveLineStart);
        p.apply(Action::Insert('X'));
        assert_eq!(p.query(), "Xtoken");
        p.apply(Action::DeleteCharForward);
        assert_eq!(p.query(), "Xoken");
        p.apply(Action::DeleteCharBack);
        assert_eq!(p.query(), "oken");
    }

    #[test]
    fn delete_word_back_stops_at_the_previous_word() {
        let mut p = picker();
        type_query(&mut p, "gh sonar   token");
        p.apply(Action::DeleteWordBack);
        assert_eq!(p.query(), "gh sonar   ");
        p.apply(Action::DeleteWordBack);
        assert_eq!(p.query(), "gh ");
        p.apply(Action::DeleteWordBack);
        assert_eq!(p.query(), "");
        p.apply(Action::DeleteWordBack);
        assert_eq!(p.query(), "");
    }

    #[test]
    fn delete_word_back_only_touches_text_before_the_caret() {
        let mut p = picker();
        type_query(&mut p, "gh sonar");
        p.apply(Action::MoveCharLeft);
        p.apply(Action::DeleteWordBack);
        assert_eq!(p.query(), "gh r");
        assert_eq!(p.caret(), 3);
    }

    #[test]
    fn line_deletes_split_the_query_at_the_caret() {
        let mut p = picker();
        type_query(&mut p, "sonarcloud");
        p.apply(Action::MoveLineStart);
        p.apply(Action::MoveCharRight);
        p.apply(Action::DeleteToLineEnd);
        assert_eq!(p.query(), "s");

        type_query(&mut p, "onar");
        p.apply(Action::MoveCharLeft);
        p.apply(Action::DeleteToLineStart);
        assert_eq!(p.query(), "r");
        assert_eq!(p.caret(), 0);
    }

    #[test]
    fn filtering_ranks_the_best_match_first() {
        let mut p = picker();
        type_query(&mut p, "sonar");
        assert_eq!(p.matches().first(), Some(&1));
        assert!(!p.matches().contains(&2));

        p.apply(Action::DeleteWordBack);
        assert_eq!(p.matches(), [0, 1, 2]);
    }

    #[test]
    fn labels_are_searchable() {
        let mut p = Picker::new(vec![
            row(&[
                "GitHub (personal)",
                "GH_TOKEN",
                "api-keys/github/credential",
            ]),
            row(&["GitHub (work)", "GH_TOKEN", "work/github/credential"]),
        ]);
        type_query(&mut p, "ghwork");
        assert_eq!(p.matches(), [1]);
    }

    #[test]
    fn matched_chars_are_reported_per_cell() {
        let mut p = picker();
        assert_eq!(p.matched_chars(1), [vec![], vec![], vec![]]);

        type_query(&mut p, "'SONAR 'personal");
        assert_eq!(
            p.matched_chars(1),
            [vec![0, 1, 2, 3, 4], vec![0, 1, 2, 3, 4, 5, 6, 7], vec![]]
        );
    }

    #[test]
    fn matched_chars_ignore_empty_cells() {
        let mut p = Picker::new(vec![row(&["ab", "", "cd"])]);
        type_query(&mut p, "'cd");
        assert_eq!(p.matched_chars(0), [vec![], vec![], vec![0, 1]]);
    }

    #[test]
    fn selection_survives_filtering() {
        let mut p = picker();
        type_query(&mut p, "sonar");
        p.apply(Action::ToggleHighlighted);
        type_query(&mut p, "zzz");
        assert!(p.matches().is_empty());
        assert_eq!(p.selection(), vec![1]);
    }

    #[test]
    fn toggling_without_matches_is_a_no_op() {
        let mut p = picker();
        type_query(&mut p, "zzz");
        assert_eq!(p.apply(Action::ToggleHighlighted), Flow::Continue);
        assert!(p.selection().is_empty());
    }

    #[test]
    fn highlight_stays_within_the_match_list() {
        let mut p = picker();
        p.apply(Action::HighlightUp);
        assert_eq!(p.highlight(), 0);
        for _ in 0..10 {
            p.apply(Action::HighlightDown);
        }
        assert_eq!(p.highlight(), 2);

        type_query(&mut p, "sonar");
        assert_eq!(p.highlight(), 0);
    }

    #[test]
    fn enter_accepts_and_esc_cancels() {
        let mut p = picker();
        assert_eq!(p.apply(Action::Accept), Flow::Accept);
        assert_eq!(p.apply(Action::Cancel), Flow::Cancel);
        assert_eq!(
            action_for(key(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Action::Cancel)
        );
        assert_eq!(action_for(ctrl('c')), Some(Action::Cancel));
    }

    #[test]
    fn key_releases_and_unbound_keys_produce_no_action() {
        let release =
            KeyEvent::new_with_kind(KeyCode::Tab, KeyModifiers::NONE, KeyEventKind::Release);
        assert_eq!(action_for(release), None);
        assert_eq!(action_for(key(KeyCode::F(1), KeyModifiers::NONE)), None);
    }

    #[test]
    fn clamp_scroll_moves_the_viewport_by_the_minimum() {
        assert_eq!(clamp_scroll(0, 3, 5), 0);
        assert_eq!(clamp_scroll(0, 5, 5), 1);
        assert_eq!(clamp_scroll(4, 2, 5), 2);
        assert_eq!(clamp_scroll(4, 4, 5), 4);
        assert_eq!(clamp_scroll(3, 0, 0), 0);
    }
}
