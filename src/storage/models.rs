use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProjectType {
    Git,
    Rust,
    Node,
    Python,
    Docker,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Directory {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub last_seen: DateTime<Utc>,
    pub project_type: ProjectType,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VisitEvent {
    pub path_id: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryMapping {
    pub query: String,
    pub path_id: u64,
    pub count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tag {
    pub name: String,
    pub path_id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Workspace {
    pub path: PathBuf,
}
