use crate::core::search::SearchResult;
use crate::storage::db::Storage;
use anyhow::{Result, bail};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, size},
};
use std::cmp;
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Read, Write};

const MAX_VISIBLE_ITEMS: usize = 8;
const EVENT_POLL_MS: u64 = 100;

pub struct App<'a> {
    pub query: String,
    pub query_cursor: usize,
    pub yank_buffer: String,
    pub results: Vec<SearchResult>,
    pub selected_index: Option<usize>,
    pub selected_path: Option<String>,
    pub cached_directories: Vec<crate::storage::models::Directory>,
    pub cached_visits: Vec<crate::storage::models::VisitEvent>,
    pub cached_mappings: Vec<crate::storage::models::QueryMapping>,
    pub cached_tags: Vec<crate::storage::models::Tag>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> App<'a> {
    pub fn new(query: String, storage: &'a Storage) -> Result<App<'a>> {
        let all_dirs = storage.list_directories()?;
        let mut cached_directories = Vec::new();

        for dir in all_dirs {
            if dir.path.exists() {
                cached_directories.push(dir);
            } else {
                let _ = storage.remove_directory(dir.id);
            }
        }

        let cached_visits = storage.list_visits()?;
        let cached_mappings = storage.get_query_mappings()?;
        let cached_tags = storage.list_tags()?;

        let initial_cursor = query.chars().count();
        let mut app = App {
            query,
            query_cursor: initial_cursor,
            yank_buffer: String::new(),
            results: Vec::new(),
            selected_index: None,
            selected_path: None,
            cached_directories,
            cached_visits,
            cached_mappings,
            cached_tags,
            _marker: std::marker::PhantomData,
        };
        app.update_search()?;
        Ok(app)
    }

    fn char_len(&self) -> usize {
        self.query.chars().count()
    }

    fn byte_index_for_cursor(&self) -> usize {
        if self.query_cursor == 0 {
            return 0;
        }
        self.query
            .char_indices()
            .nth(self.query_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.query.len())
    }

    fn clamp_cursor(&mut self) {
        self.query_cursor = self.query_cursor.min(self.char_len());
    }

    pub fn update_search(&mut self) -> Result<()> {
        self.clamp_cursor();
        self.results = self.perform_search()?;
        self.selected_index = if self.results.is_empty() {
            None
        } else {
            Some(0)
        };
        Ok(())
    }

    pub fn query_with_cursor_marker(&self) -> String {
        let chars: Vec<char> = self.query.chars().collect();
        let cursor = self.query_cursor.min(chars.len());
        let mut out = String::with_capacity(self.query.len() + 1);
        for (i, ch) in chars.iter().enumerate() {
            if i == cursor {
                out.push('|');
            }
            out.push(*ch);
        }
        if cursor == chars.len() {
            out.push('|');
        }
        out
    }

    fn move_left(&mut self) {
        if self.query_cursor > 0 {
            self.query_cursor -= 1;
        }
    }

    fn move_right(&mut self) {
        if self.query_cursor < self.char_len() {
            self.query_cursor += 1;
        }
    }

    fn move_home(&mut self) {
        self.query_cursor = 0;
    }

    fn move_end(&mut self) {
        self.query_cursor = self.char_len();
    }

    fn insert_char(&mut self, c: char) {
        let idx = self.byte_index_for_cursor();
        self.query.insert(idx, c);
        self.query_cursor += 1;
    }

    fn backspace(&mut self) -> bool {
        if self.query_cursor == 0 {
            return false;
        }
        let chars: Vec<char> = self.query.chars().collect();
        let remove_idx = self.query_cursor - 1;
        let mut out = String::with_capacity(self.query.len());
        for (i, ch) in chars.iter().enumerate() {
            if i != remove_idx {
                out.push(*ch);
            }
        }
        self.query = out;
        self.query_cursor -= 1;
        true
    }

    fn delete_forward(&mut self) -> bool {
        let len = self.char_len();
        if self.query_cursor >= len {
            return false;
        }
        let chars: Vec<char> = self.query.chars().collect();
        let mut out = String::with_capacity(self.query.len());
        for (i, ch) in chars.iter().enumerate() {
            if i != self.query_cursor {
                out.push(*ch);
            }
        }
        self.query = out;
        true
    }

    fn clear_query(&mut self) -> bool {
        if self.query.is_empty() {
            return false;
        }
        self.query.clear();
        self.query_cursor = 0;
        true
    }

    fn delete_word_backward(&mut self) -> bool {
        if self.query_cursor == 0 {
            return false;
        }
        let chars: Vec<char> = self.query.chars().collect();
        let mut start = self.query_cursor;

        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }

        self.yank_buffer = chars[start..self.query_cursor].iter().collect();

        let mut out = String::with_capacity(self.query.len());
        for (i, ch) in chars.iter().enumerate() {
            if i < start || i >= self.query_cursor {
                out.push(*ch);
            }
        }
        self.query = out;
        self.query_cursor = start;
        true
    }

    fn move_word_forward(&mut self) {
        let chars: Vec<char> = self.query.chars().collect();
        let len = chars.len();
        let mut i = self.query_cursor.min(len);

        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        while i < len && !chars[i].is_whitespace() {
            i += 1;
        }

        self.query_cursor = i;
    }

    fn move_word_backward(&mut self) {
        if self.query_cursor == 0 {
            return;
        }

        let chars: Vec<char> = self.query.chars().collect();
        let mut i = self.query_cursor.min(chars.len());

        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }

        self.query_cursor = i;
    }

    fn kill_to_end(&mut self) -> bool {
        let len = self.char_len();
        if self.query_cursor >= len {
            self.yank_buffer.clear();
            return false;
        }
        let chars: Vec<char> = self.query.chars().collect();
        self.yank_buffer = chars[self.query_cursor..].iter().collect();
        self.query = chars[..self.query_cursor].iter().collect();
        true
    }

    fn kill_word_forward(&mut self) -> bool {
        let chars: Vec<char> = self.query.chars().collect();
        let len = chars.len();
        let start = self.query_cursor.min(len);
        if start >= len {
            self.yank_buffer.clear();
            return false;
        }

        let mut end = start;
        while end < len && chars[end].is_whitespace() {
            end += 1;
        }
        while end < len && !chars[end].is_whitespace() {
            end += 1;
        }

        if end == start {
            return false;
        }

        self.yank_buffer = chars[start..end].iter().collect();

        let mut out = String::with_capacity(self.query.len());
        for (i, ch) in chars.iter().enumerate() {
            if i < start || i >= end {
                out.push(*ch);
            }
        }
        self.query = out;
        true
    }

    fn yank(&mut self) -> bool {
        if self.yank_buffer.is_empty() {
            return false;
        }
        let idx = self.byte_index_for_cursor();
        self.query.insert_str(idx, &self.yank_buffer);
        self.query_cursor += self.yank_buffer.chars().count();
        true
    }

    fn perform_search(&self) -> Result<Vec<SearchResult>> {
        use crate::core::ranking::*;
        use std::collections::HashMap;

        let mut frequency_map: HashMap<u64, u64> = HashMap::new();
        let mut recency_map: HashMap<u64, chrono::DateTime<chrono::Utc>> = HashMap::new();

        for visit in &self.cached_visits {
            *frequency_map.entry(visit.path_id).or_insert(0) += 1;
            let entry = recency_map.entry(visit.path_id).or_insert(visit.timestamp);
            if visit.timestamp > *entry {
                *entry = visit.timestamp;
            }
        }

        let max_freq = frequency_map.values().max().cloned().unwrap_or(1) as f64;
        let max_learned = self
            .cached_mappings
            .iter()
            .map(|m| m.count)
            .max()
            .unwrap_or(1) as f64;

        let mut results = Vec::new();

        let tag_matches: HashMap<u64, bool> = if self.query.starts_with('@') {
            let tag_name = self
                .query
                .split_whitespace()
                .next()
                .unwrap_or("@")
                .trim_start_matches('@');
            self.cached_tags
                .iter()
                .filter(|t| t.name == tag_name)
                .map(|t| (t.path_id, true))
                .collect()
        } else {
            HashMap::new()
        };

        for dir in &self.cached_directories {
            let path_str = dir.path.to_string_lossy();

            if !crate::core::matcher::match_path(&path_str, &self.query) {
                continue;
            }

            let fuzzy_score = 1.0f64;
            let is_tag_match = tag_matches.contains_key(&dir.id);
            let effective_fuzzy =
                (fuzzy_score + if is_tag_match { 1.0f64 } else { 0.0f64 }).min(1.0f64);
            let project_bonus = get_project_bonus(&dir.project_type);

            let score = calculate_score(
                effective_fuzzy,
                get_recency_score(recency_map.get(&dir.id).cloned()),
                frequency_map.get(&dir.id).cloned().unwrap_or(0) as f64 / max_freq,
                self.cached_mappings
                    .iter()
                    .find(|m| m.query == self.query && m.path_id == dir.id)
                    .map(|m| m.count as f64 / max_learned)
                    .unwrap_or(0.0),
                project_bonus,
                &RankingWeights::default(),
            );

            results.push(SearchResult {
                directory: dir.clone(),
                score,
            });
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(results)
    }

    pub fn next(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let i = match self.selected_index {
            Some(i) => {
                if i >= self.results.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.selected_index = Some(i);
    }

    pub fn previous(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let i = match self.selected_index {
            Some(i) => {
                if i == 0 {
                    self.results.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.selected_index = Some(i);
    }

    fn apply_key_editing(&mut self, key: KeyEvent) -> Result<bool> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        match key.code {
            KeyCode::Left => {
                self.move_left();
                Ok(false)
            }
            KeyCode::Right => {
                self.move_right();
                Ok(false)
            }
            KeyCode::Backspace => {
                if self.backspace() {
                    self.update_search()?;
                }
                Ok(false)
            }
            KeyCode::Delete => {
                if self.delete_forward() {
                    self.update_search()?;
                }
                Ok(false)
            }
            KeyCode::Char('a') if ctrl => {
                self.move_home();
                Ok(false)
            }
            KeyCode::Char('e') if ctrl => {
                self.move_end();
                Ok(false)
            }
            KeyCode::Char('b') if ctrl => {
                self.move_left();
                Ok(false)
            }
            KeyCode::Char('f') if ctrl => {
                self.move_right();
                Ok(false)
            }
            KeyCode::Char('u') if ctrl => {
                if self.clear_query() {
                    self.update_search()?;
                }
                Ok(false)
            }
            KeyCode::Char('w') if ctrl => {
                if self.delete_word_backward() {
                    self.update_search()?;
                }
                Ok(false)
            }
            KeyCode::Char('k') if ctrl => {
                if self.kill_to_end() {
                    self.update_search()?;
                }
                Ok(false)
            }
            KeyCode::Char('d') if ctrl => {
                if self.delete_forward() {
                    self.update_search()?;
                }
                Ok(false)
            }
            KeyCode::Char('y') if ctrl => {
                if self.yank() {
                    self.update_search()?;
                }
                Ok(false)
            }
            KeyCode::Char('b') if alt => {
                self.move_word_backward();
                Ok(false)
            }
            KeyCode::Char('f') if alt => {
                self.move_word_forward();
                Ok(false)
            }
            KeyCode::Char('d') if alt => {
                if self.kill_word_forward() {
                    self.update_search()?;
                }
                Ok(false)
            }
            KeyCode::Char(c) if !ctrl && !alt => {
                self.insert_char(c);
                self.update_search()?;
                Ok(false)
            }
            _ => Ok(false),
        }
    }
}

#[derive(Clone, Copy)]
enum PopupMode {
    Below,
    Above,
}

#[derive(Clone, Copy)]
struct PopupLayout {
    top_row: u16,
    height: u16,
    mode: PopupMode,
}

fn move_cursor(tty: &mut std::fs::File, col: u16, row: u16) -> io::Result<()> {
    write!(tty, "\x1b[{};{}H", row + 1, col + 1)
}

fn query_cursor_position(tty: &mut std::fs::File) -> io::Result<(u16, u16)> {
    tty.write_all(b"\x1b[6n")?;
    tty.flush()?;

    let mut response = Vec::with_capacity(16);
    let mut buf = [0u8; 1];

    loop {
        tty.read_exact(&mut buf)?;
        response.push(buf[0]);
        if buf[0] == b'R' {
            break;
        }
        if response.len() > 64 {
            return Err(io::Error::other("cursor position response too long"));
        }
    }

    let s = String::from_utf8_lossy(&response);
    let s = s.trim_start_matches("\x1b[").trim_end_matches('R');
    let mut parts = s.split(';');
    let row = parts
        .next()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(1);
    let col = parts
        .next()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(1);

    Ok((col.saturating_sub(1), row.saturating_sub(1)))
}

fn text_width(input: &str) -> usize {
    input.chars().count()
}

fn truncate_for_width(input: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= width {
        return input.to_string();
    }

    if width <= 3 {
        return ".".repeat(width);
    }

    let mut out: String = chars[..(width - 3)].iter().collect();
    out.push_str("...");
    out
}

fn pad_to_width(input: &str, width: usize) -> String {
    let clipped = truncate_for_width(input, width);
    let clipped_width = clipped.chars().count();
    if clipped_width >= width {
        return clipped;
    }
    let mut out = String::with_capacity(width);
    out.push_str(&clipped);
    out.push_str(&" ".repeat(width - clipped_width));
    out
}

fn format_input_row(left: &str, right: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let right_w = text_width(right);
    if right_w >= width {
        return truncate_for_width(right, width);
    }

    let min_gap = 1usize;
    let max_left = width.saturating_sub(right_w + min_gap);
    let left_part = truncate_for_width(left, max_left);
    let left_w = text_width(&left_part);
    let gap = width.saturating_sub(left_w + right_w);
    format!("{left_part}{}{right}", " ".repeat(gap))
}

fn normalized_query_tokens(query: &str) -> Vec<Vec<char>> {
    query
        .split_whitespace()
        .filter_map(|token| {
            let trimmed = token.trim_start_matches('^').trim_end_matches('$');
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_lowercase().chars().collect::<Vec<char>>())
            }
        })
        .collect()
}

fn match_marks_for_line(line: &str, query: &str) -> Vec<bool> {
    let chars: Vec<char> = line.chars().collect();
    let lower_chars: Vec<char> = chars.iter().flat_map(|c| c.to_lowercase()).collect();
    let mut marks = vec![false; chars.len()];
    let tokens = normalized_query_tokens(query);

    if tokens.is_empty() || chars.is_empty() {
        return marks;
    }

    for token in tokens {
        let token_len = token.len();
        if token_len == 0 || token_len > lower_chars.len() {
            continue;
        }

        for start in 0..=(lower_chars.len() - token_len) {
            let mut matched = true;
            for i in 0..token_len {
                if lower_chars[start + i] != token[i] {
                    matched = false;
                    break;
                }
            }
            if matched {
                for i in start..(start + token_len) {
                    if i < marks.len() {
                        marks[i] = true;
                    }
                }
            }
        }
    }

    marks
}

fn styled_line_with_matches(line: &str, query: &str, selected: bool) -> String {
    let chars: Vec<char> = line.chars().collect();
    let marks = match_marks_for_line(line, query);
    let mut out = String::new();

    let base_style = if selected { "\x1b[36m" } else { "\x1b[0m" };
    let match_style = "\x1b[1;33m";

    let mut current_match = false;
    out.push_str(base_style);
    for (i, ch) in chars.iter().enumerate() {
        let is_match = marks.get(i).copied().unwrap_or(false);
        if is_match != current_match {
            out.push_str(if is_match { match_style } else { base_style });
            current_match = is_match;
        }
        out.push(*ch);
    }
    out.push_str("\x1b[0m");
    out
}

fn format_result_line(res: &SearchResult, selected: bool) -> String {
    let project_icon = match res.directory.project_type {
        crate::storage::models::ProjectType::Git => "[git]",
        crate::storage::models::ProjectType::Rust => "[rs]",
        crate::storage::models::ProjectType::Node => "[js]",
        crate::storage::models::ProjectType::Python => "[py]",
        crate::storage::models::ProjectType::Docker => "[dk]",
        crate::storage::models::ProjectType::Unknown => "[dir]",
    };
    let prefix = if selected { "> " } else { "  " };
    format!(
        "{}{} {}  {}",
        prefix,
        project_icon,
        res.directory.name,
        res.directory.path.display()
    )
}

fn compute_layout(rows: u16, anchor_row: u16, desired_height: u16) -> PopupLayout {
    let above_space = anchor_row;
    let below_space = rows.saturating_sub(anchor_row + 1);

    if below_space >= desired_height {
        return PopupLayout {
            top_row: anchor_row + 1,
            height: desired_height,
            mode: PopupMode::Below,
        };
    }

    let height = cmp::max(1, above_space.min(desired_height));
    PopupLayout {
        top_row: anchor_row.saturating_sub(height),
        height,
        mode: PopupMode::Above,
    }
}

fn clear_layout(tty: &mut std::fs::File, layout: PopupLayout) -> io::Result<()> {
    for offset in 0..layout.height {
        move_cursor(tty, 0, layout.top_row + offset)?;
        write!(tty, "\x1b[0m\x1b[2K")?;
    }
    tty.flush()
}

fn draw_popup(
    tty: &mut std::fs::File,
    app: &App<'_>,
    anchor_col: u16,
    anchor_row: u16,
    prev_layout: Option<PopupLayout>,
) -> io::Result<PopupLayout> {
    let (cols, rows) = size()?;
    let max_width = cols.max(1) as usize;

    let desired_items = app.results.len().clamp(1, MAX_VISIBLE_ITEMS);
    let desired_height = cmp::max(1, 1 + desired_items as u16);
    let layout = compute_layout(rows, anchor_row, desired_height);

    if let Some(old) = prev_layout {
        clear_layout(tty, old)?;
    }

    let visible_items = layout.height.saturating_sub(1) as usize;
    let selected = app.selected_index.unwrap_or(0);
    let start = if app.results.len() <= visible_items || visible_items == 0 {
        0
    } else {
        let centered = selected.saturating_sub(visible_items / 2);
        cmp::min(centered, app.results.len() - visible_items)
    };

    let mut list_lines = Vec::new();
    if app.results.is_empty() {
        list_lines.push("  (no matches)".to_string());
    } else {
        for line_idx in 0..visible_items {
            let idx = start + line_idx;
            if idx >= app.results.len() {
                break;
            }
            list_lines.push(format_result_line(
                &app.results[idx],
                Some(idx) == app.selected_index,
            ));
        }
    }

    let input_left = format!(">:{}", app.query_with_cursor_marker());
    let input_right = format!("{}/{}", app.results.len(), app.cached_directories.len());
    let mut lines = Vec::new();
    match layout.mode {
        PopupMode::Below => {
            lines.push(input_left.clone());
            lines.extend(list_lines);
        }
        PopupMode::Above => {
            lines.extend(list_lines);
            lines.push(input_left.clone());
        }
    }

    let content_width = lines.iter().map(|line| text_width(line)).max().unwrap_or(1);
    let popup_width = content_width.clamp(1, max_width) as u16;

    for (i, line) in lines.iter().enumerate() {
        let row = layout.top_row + i as u16;
        if row >= layout.top_row + layout.height {
            break;
        }

        let padded = pad_to_width(line, popup_width as usize);
        move_cursor(tty, 0, row)?;
        write!(tty, "\x1b[0m\x1b[2K")?;

        let is_input_row = match layout.mode {
            PopupMode::Below => i == 0,
            PopupMode::Above => i + 1 == lines.len(),
        };

        if is_input_row {
            let aligned = format_input_row(&input_left, &input_right, popup_width as usize);
            let aligned = pad_to_width(&aligned, popup_width as usize);
            write!(tty, "\x1b[7m{aligned}\x1b[0m")?;
        } else {
            let selected_row = padded.starts_with('>');
            let styled = styled_line_with_matches(&padded, &app.query, selected_row);
            write!(tty, "{styled}")?;
        }
    }

    move_cursor(tty, anchor_col, anchor_row)?;
    tty.flush()?;

    Ok(layout)
}

pub fn run_ui(storage: &Storage, initial_query: String) -> Result<Option<String>> {
    if !io::stdin().is_terminal() {
        bail!("Interactive mode requires a TTY on stdin. Use --auto for scripting.");
    }

    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|e| anyhow::anyhow!("Interactive mode requires a terminal (/dev/tty): {e}"))?;

    enable_raw_mode()?;

    let mut anchor_pos: Option<(u16, u16)> = None;
    let mut last_layout: Option<PopupLayout> = None;
    let mut cursor_hidden = false;

    let run_result = (|| -> Result<Option<String>> {
        let (anchor_col, anchor_row) = query_cursor_position(&mut tty)?;
        anchor_pos = Some((anchor_col, anchor_row));

        write!(tty, "\x1b[?25l")?;
        tty.flush()?;
        cursor_hidden = true;

        let mut app = App::new(initial_query, storage)?;
        let mut needs_redraw = true;

        loop {
            if needs_redraw {
                last_layout = Some(draw_popup(
                    &mut tty,
                    &app,
                    anchor_col,
                    anchor_row,
                    last_layout,
                )?);
                needs_redraw = false;
            }

            if event::poll(std::time::Duration::from_millis(EVENT_POLL_MS))?
                && let Event::Key(key) = event::read()?
            {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    KeyCode::Esc => {
                        return Ok(None);
                    }
                    KeyCode::Enter => {
                        if let Some(i) = app.selected_index {
                            app.selected_path =
                                Some(app.results[i].directory.path.to_string_lossy().into_owned());
                        }
                        return Ok(app.selected_path.clone());
                    }
                    KeyCode::Up => {
                        app.previous();
                        needs_redraw = true;
                    }
                    KeyCode::Down => {
                        app.next();
                        needs_redraw = true;
                    }
                    KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.next();
                        needs_redraw = true;
                    }
                    KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.next();
                        needs_redraw = true;
                    }
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.previous();
                        needs_redraw = true;
                    }
                    _ => {
                        app.apply_key_editing(key)?;
                        needs_redraw = true;
                    }
                }
            }
        }
    })();

    if let Some(layout) = last_layout {
        let _ = clear_layout(&mut tty, layout);
    }
    if let Some((anchor_col, anchor_row)) = anchor_pos {
        let _ = move_cursor(&mut tty, anchor_col, anchor_row);
    }
    if cursor_hidden {
        let _ = write!(tty, "\x1b[?25h");
    }
    let _ = tty.flush();
    let _ = disable_raw_mode();

    run_result
}
