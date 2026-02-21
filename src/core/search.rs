use crate::core::ranking::*;
use crate::storage::db::Storage;
use crate::storage::models::*;
use anyhow::Result;
use std::collections::HashMap;

pub struct SearchResult {
    pub directory: Directory,
    pub score: f64,
}

pub fn search(storage: &Storage, query: &str) -> Result<Vec<SearchResult>> {
    let directories = storage.list_directories()?;
    let visits = storage.list_visits()?;
    let mappings = storage.get_query_mappings()?;
    let tags = storage.list_tags()?;

    let mut frequency_map: HashMap<u64, u64> = HashMap::new();
    let mut recency_map: HashMap<u64, chrono::DateTime<chrono::Utc>> = HashMap::new();

    for visit in visits {
        *frequency_map.entry(visit.path_id).or_insert(0) += 1;
        let entry = recency_map.entry(visit.path_id).or_insert(visit.timestamp);
        if visit.timestamp > *entry {
            *entry = visit.timestamp;
        }
    }

    let max_freq = frequency_map.values().max().cloned().unwrap_or(1) as f64;
    let max_learned = mappings.iter().map(|m| m.count).max().unwrap_or(1) as f64;

    let mut results = Vec::new();

    let tag_matches: HashMap<u64, bool> = if query.starts_with('@') {
        let tag_name = query
            .split_whitespace()
            .next()
            .unwrap_or("@")
            .trim_start_matches('@');
        tags.iter()
            .filter(|t| t.name == tag_name)
            .map(|t| (t.path_id, true))
            .collect()
    } else {
        HashMap::new()
    };

    for dir in directories {
        let path_str = dir.path.to_string_lossy();

        if !crate::core::matcher::match_path(&path_str, query) {
            continue;
        }

        let fuzzy_score = 1.0f64;

        let is_tag_match = tag_matches.contains_key(&dir.id);

        let recency_score = get_recency_score(recency_map.get(&dir.id).cloned());
        let frequency_score = frequency_map.get(&dir.id).cloned().unwrap_or(0) as f64 / max_freq;

        let learned_score = mappings
            .iter()
            .find(|m| m.query == query && m.path_id == dir.id)
            .map(|m| m.count as f64 / max_learned)
            .unwrap_or(0.0);

        let tag_bonus = if is_tag_match { 1.0f64 } else { 0.0f64 };

        let effective_fuzzy = (fuzzy_score + tag_bonus).min(1.0f64);

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
            directory: dir,
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
