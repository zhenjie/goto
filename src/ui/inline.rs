use crate::core::search::SearchResult;
use crate::storage::db::Storage;
use anyhow::{Result, bail};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, size},
};
use std::cmp;
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Read, Write};

const MAX_VISIBLE_ITEMS: usize = 8;
const EVENT_POLL_MS: u64 = 100;

pub struct App<'a> {
    pub query: String,
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

        let mut app = App {
            query,
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

    pub fn update_search(&mut self) -> Result<()> {
        self.results = self.perform_search()?;
        self.selected_index = if self.results.is_empty() {
            None
        } else {
            Some(0)
        };
        Ok(())
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
}

#[derive(Clone, Copy)]
struct PopupLayout {
    top_row: u16,
    height: u16,
}

fn move_cursor(tty: &mut std::fs::File, col: u16, row: u16) -> io::Result<()> {
    write!(tty, "\x1b[{};{}H", row + 1, col + 1)
}

fn clear_line(tty: &mut std::fs::File) -> io::Result<()> {
    write!(tty, "\x1b[2K")
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

fn format_result_line(res: &SearchResult) -> String {
    let project_icon = match res.directory.project_type {
        crate::storage::models::ProjectType::Git => "[git]",
        crate::storage::models::ProjectType::Rust => "[rs]",
        crate::storage::models::ProjectType::Node => "[js]",
        crate::storage::models::ProjectType::Python => "[py]",
        crate::storage::models::ProjectType::Docker => "[dk]",
        crate::storage::models::ProjectType::Unknown => "[dir]",
    };
    format!(
        "{} {}  {}",
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
        };
    }

    if above_space >= desired_height {
        return PopupLayout {
            top_row: anchor_row - desired_height,
            height: desired_height,
        };
    }

    if below_space >= above_space {
        PopupLayout {
            top_row: anchor_row + 1,
            height: cmp::max(1, below_space),
        }
    } else {
        let height = cmp::max(1, above_space);
        PopupLayout {
            top_row: anchor_row.saturating_sub(height),
            height,
        }
    }
}

fn clear_layout(tty: &mut std::fs::File, layout: PopupLayout) -> io::Result<()> {
    for offset in 0..layout.height {
        move_cursor(tty, 0, layout.top_row + offset)?;
        clear_line(tty)?;
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
    let width = cols as usize;

    let desired_items = app.results.len().clamp(1, MAX_VISIBLE_ITEMS);
    let desired_height = cmp::max(1, 1 + desired_items as u16);
    let layout = compute_layout(rows, anchor_row, desired_height);

    if let Some(old) = prev_layout {
        clear_layout(tty, old)?;
    }

    for offset in 0..layout.height {
        move_cursor(tty, 0, layout.top_row + offset)?;
        clear_line(tty)?;
    }

    move_cursor(tty, 0, layout.top_row)?;
    let header = truncate_for_width(&format!("goto: {}", app.query), width);
    write!(tty, "\x1b[7m{header}\x1b[0m")?;

    if layout.height > 1 {
        let visible_items = (layout.height - 1) as usize;
        let selected = app.selected_index.unwrap_or(0);

        let start = if app.results.len() <= visible_items {
            0
        } else {
            let centered = selected.saturating_sub(visible_items / 2);
            cmp::min(centered, app.results.len() - visible_items)
        };

        for line_idx in 0..visible_items {
            let row = layout.top_row + 1 + line_idx as u16;
            move_cursor(tty, 0, row)?;

            if app.results.is_empty() {
                let empty = truncate_for_width("  (no matches)", width);
                write!(tty, "{empty}")?;
                break;
            }

            let item_idx = start + line_idx;
            if item_idx >= app.results.len() {
                break;
            }

            let prefix = if Some(item_idx) == app.selected_index {
                "> "
            } else {
                "  "
            };
            let line = format!("{}{}", prefix, format_result_line(&app.results[item_idx]));
            let line = truncate_for_width(&line, width);
            if Some(item_idx) == app.selected_index {
                write!(tty, "\x1b[36m{line}\x1b[0m")?;
            } else {
                write!(tty, "{line}")?;
            }
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

        loop {
            last_layout = Some(draw_popup(
                &mut tty,
                &app,
                anchor_col,
                anchor_row,
                last_layout,
            )?);

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
                    KeyCode::Up | KeyCode::Char('k')
                        if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        app.previous();
                    }
                    KeyCode::Down | KeyCode::Char('j')
                        if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        app.next();
                    }
                    KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.previous();
                    }
                    KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.next();
                    }
                    KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.next();
                    }
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.previous();
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.query.clear();
                        app.update_search()?;
                    }
                    KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(pos) = app.query.rfind(' ') {
                            app.query.truncate(pos);
                        } else {
                            app.query.clear();
                        }
                        app.update_search()?;
                    }
                    KeyCode::Backspace => {
                        app.query.pop();
                        app.update_search()?;
                    }
                    KeyCode::Char(c) => {
                        app.query.push(c);
                        app.update_search()?;
                    }
                    _ => {}
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
