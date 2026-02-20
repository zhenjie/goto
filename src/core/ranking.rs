use crate::storage::models::*;
use chrono::{DateTime, Utc};

pub struct RankingWeights {
    pub fuzzy: f64,
    pub recency: f64,
    pub frequency: f64,
    pub learned: f64,
    pub project: f64,
}

impl Default for RankingWeights {
    fn default() -> Self {
        Self {
            fuzzy: 0.35,
            recency: 0.25,
            frequency: 0.20,
            learned: 0.15,
            project: 0.05,
        }
    }
}

pub fn calculate_score(
    fuzzy_score: f64,
    recency_score: f64,
    frequency_score: f64,
    learned_score: f64,
    project_bonus: f64,
    weights: &RankingWeights,
) -> f64 {
    weights.fuzzy * fuzzy_score +
    weights.recency * recency_score +
    weights.frequency * frequency_score +
    weights.learned * learned_score +
    weights.project * project_bonus
}

pub fn get_recency_score(last_visited: Option<DateTime<Utc>>) -> f64 {
    match last_visited {
        Some(dt) => {
            let now = Utc::now();
            let duration = now.signed_duration_since(dt);
            let hours = duration.num_hours() as f64;
            // Decay function: 1 / (1 + hours/24)
            1.0 / (1.0 + hours / 24.0)
        }
        None => 0.0,
    }
}

pub fn get_project_bonus(project_type: &ProjectType) -> f64 {
    match project_type {
        ProjectType::Unknown => 0.0,
        _ => 1.0,
    }
}
