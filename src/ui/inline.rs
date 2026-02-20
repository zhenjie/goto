use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Terminal, Viewport,
};
use std::io;
use crate::core::search::SearchResult;
use crate::storage::db::Storage;

pub struct App<'a> {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub state: ListState,
    pub selected_path: Option<String>,
    // Cached data for faster searching
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
                // Background cleanup if discovered here
                let _ = storage.remove_directory(dir.id);
            }
        }

        let cached_visits = storage.list_visits()?;
        let cached_mappings = storage.get_query_mappings()?;
        let cached_tags = storage.list_tags()?;

        let mut app = App {
            query,
            results: Vec::new(),
            state: ListState::default(),
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
        // Perform search using cached data instead of DB reads
        self.results = self.perform_search()?;
        if self.results.is_empty() {
            self.state.select(None);
        } else {
            self.state.select(Some(0));
        }
        Ok(())
    }

    fn perform_search(&self) -> Result<Vec<SearchResult>> {
        use crate::core::ranking::*;
        use fuzzy_matcher::FuzzyMatcher;
        use fuzzy_matcher::skim::SkimMatcherV2;
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
        let max_learned = self.cached_mappings.iter().map(|m| m.count).max().unwrap_or(1) as f64;
        
        let matcher = SkimMatcherV2::default();
        let mut results = Vec::new();

        let query_tokens: Vec<&str> = self.query.split_whitespace().collect();

        let tag_matches: HashMap<u64, bool> = if self.query.starts_with('@') {
            let tag_name = self.query.split_whitespace().next().unwrap_or("@").trim_start_matches('@');
            self.cached_tags.iter().filter(|t| t.name == tag_name).map(|t| (t.path_id, true)).collect()
        } else {
            HashMap::new()
        };

        for dir in &self.cached_directories {
            let path_str = dir.path.to_string_lossy();
            
            let mut total_fuzzy_score = 0;
            let mut all_tokens_matched = true;

            if !query_tokens.is_empty() {
                for token in &query_tokens {
                    if let Some(score) = matcher.fuzzy_match(&path_str, token) {
                        total_fuzzy_score += score;
                    } else {
                        all_tokens_matched = false;
                        break;
                    }
                }
            } else {
                total_fuzzy_score = 100; // Default score for empty query
            }

            let is_tag_match = tag_matches.contains_key(&dir.id);

            if !all_tokens_matched && !is_tag_match && !self.query.is_empty() {
                continue;
            }

            let fuzzy_score = if query_tokens.is_empty() {
                1.0
            } else {
                (total_fuzzy_score as f64 / (query_tokens.len() as f64 * 100.0)).min(1.0)
            };

            let recency_score = get_recency_score(recency_map.get(&dir.id).cloned());
            let frequency_score = frequency_map.get(&dir.id).cloned().unwrap_or(0) as f64 / max_freq;
            
            let learned_score = self.cached_mappings.iter()
                .find(|m| m.query == self.query && m.path_id == dir.id)
                .map(|m| m.count as f64 / max_learned)
                .unwrap_or(0.0);
            
            let tag_bonus = if is_tag_match { 1.0 } else { 0.0 };
            let effective_fuzzy = (fuzzy_score + tag_bonus).min(1.0);
            let project_bonus = get_project_bonus(&dir.project_type);
            
            let score = calculate_score(
                effective_fuzzy,
                recency_score,
                frequency_score,
                learned_score,
                project_bonus,
                &RankingWeights::default(),
            );

            results.push(SearchResult {
                directory: dir.clone(),
                score,
            });
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    pub fn next(&mut self) {
        if self.results.is_empty() { return; }
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.results.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.results.is_empty() { return; }
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.results.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

pub fn run_ui(storage: &Storage, initial_query: String) -> Result<Option<String>> {
    enable_raw_mode()?;
    let stdout = io::stdout();
    
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: Viewport::Inline(10),
        },
    )?;

    let mut app = App::new(initial_query, storage)?;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0)].as_ref())
                .split(f.size());

            let items: Vec<ListItem> = app.results
                .iter()
                .map(|res| {
                    let project_icon = match res.directory.project_type {
                        crate::storage::models::ProjectType::Git => "󰊢",
                        crate::storage::models::ProjectType::Rust => "🦀",
                        crate::storage::models::ProjectType::Node => "",
                        crate::storage::models::ProjectType::Python => "",
                        crate::storage::models::ProjectType::Docker => "",
                        crate::storage::models::ProjectType::Unknown => "📁",
                    };
                    let content = format!("{} {}  {}", project_icon, res.directory.name, res.directory.path.display());
                    ListItem::new(content)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!(" goto: {} ", app.query)))
                .highlight_style(
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");

            f.render_stateful_widget(list, chunks[0], &mut app.state);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    KeyCode::Esc => {
                        break;
                    }
                    KeyCode::Enter => {
                        if let Some(i) = app.state.selected() {
                            app.selected_path = Some(app.results[i].directory.path.to_string_lossy().into_owned());
                        }
                        break;
                    }
                    KeyCode::Up => {
                        app.previous();
                    }
                    KeyCode::Down => {
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
    }

    disable_raw_mode()?;
    terminal.clear()?;

    Ok(app.selected_path)
}
