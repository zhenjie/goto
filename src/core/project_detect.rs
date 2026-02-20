use std::path::Path;
use crate::storage::models::ProjectType;

pub fn detect_project_type(path: &Path) -> ProjectType {
    if path.join(".git").exists() {
        return ProjectType::Git;
    }
    if path.join("Cargo.toml").exists() {
        return ProjectType::Rust;
    }
    if path.join("package.json").exists() {
        return ProjectType::Node;
    }
    if path.join("pyproject.toml").exists() || path.join("requirements.txt").exists() {
        return ProjectType::Python;
    }
    if path.join("Dockerfile").exists() {
        return ProjectType::Docker;
    }
    ProjectType::Unknown
}
