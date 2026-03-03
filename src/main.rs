mod cli;
mod core;
mod shell;
mod storage;
mod ui;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use cli::{Cli, Commands, TagAction, WorkspaceAction};
use std::path::{Component, Path, PathBuf};
use storage::db::Storage;
use storage::models::VisitEvent;

fn is_direct_path_query(query: &str) -> bool {
    query == "."
        || query == ".."
        || query.starts_with("./")
        || query.starts_with("../")
        || query.starts_with('/')
}

fn resolve_direct_directory_query(query: &str) -> Result<Option<PathBuf>> {
    if !is_direct_path_query(query) {
        return Ok(None);
    }

    let candidate = Path::new(query);
    let abs_path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        std::env::current_dir()?.join(candidate)
    };

    if !abs_path.is_dir() {
        return Ok(None);
    }

    let canonical = std::fs::canonicalize(&abs_path).unwrap_or(abs_path);
    Ok(Some(canonical))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut has_root = false;

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => {
                normalized.push(Path::new("/"));
                has_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !has_root {
                    normalized.push("..");
                }
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }

    if normalized.as_os_str().is_empty() {
        if has_root {
            PathBuf::from("/")
        } else {
            PathBuf::from(".")
        }
    } else {
        normalized
    }
}

fn resolve_input_path_from_base(base_dir: &Path, input_path: &Path) -> PathBuf {
    let combined = if input_path.is_absolute() {
        input_path.to_path_buf()
    } else {
        base_dir.join(input_path)
    };

    std::fs::canonicalize(&combined).unwrap_or_else(|_| normalize_path(&combined))
}

fn resolve_input_path(input_path: &Path) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(resolve_input_path_from_base(&cwd, input_path))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let storage = Storage::new()?;

    match cli.command {
        Some(Commands::Workspace { action }) => match action {
            WorkspaceAction::Add { force, path } => {
                let abs_path = resolve_input_path(&path)?;
                core::workspace::add_workspace(&storage, abs_path, force)?;
                eprintln!("Workspace added. Indexing...");
                core::index::index_workspaces(&storage)?;
            }
            WorkspaceAction::Remove { path } => {
                let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
                    if path.is_absolute() {
                        path
                    } else {
                        std::env::current_dir().unwrap().join(path)
                    }
                });
                core::workspace::remove_workspace(&storage, abs_path.clone())?;
                eprintln!("Workspace removed. Cleaning up index...");
                let dirs = storage.list_directories()?;
                for d in dirs {
                    if d.path.starts_with(&abs_path) {
                        storage.remove_directory(d.id)?;
                    }
                }
            }
            WorkspaceAction::List => {
                for path in core::workspace::list_workspaces(&storage)? {
                    println!("{}", path.display());
                }
            }
        },
        Some(Commands::Index) => {
            core::index::index_workspaces(&storage)?;
        }
        Some(Commands::Tag { action }) => match action {
            TagAction::Add { tag, path } => {
                let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
                    if path.is_absolute() {
                        path
                    } else {
                        std::env::current_dir().unwrap().join(path)
                    }
                });
                // Find path ID
                let dirs = storage.list_directories()?;
                if let Some(dir) = dirs.into_iter().find(|d| d.path == abs_path) {
                    storage.add_tag(tag, dir.id)?;
                } else {
                    anyhow::bail!("Path not found in index. Run 'goto index' first.");
                }
            }
            TagAction::Remove { tag, path } => {
                let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
                    if path.is_absolute() {
                        path
                    } else {
                        std::env::current_dir().unwrap().join(path)
                    }
                });
                let dirs = storage.list_directories()?;
                if let Some(dir) = dirs.into_iter().find(|d| d.path == abs_path) {
                    storage.remove_tag(&tag, dir.id)?;
                }
            }
            TagAction::List => {
                for tag in storage.list_tags()? {
                    if let Some(dir) = storage.get_directory(tag.path_id)? {
                        println!("@{} -> {}", tag.name, dir.path.display());
                    }
                }
            }
        },
        Some(Commands::Doctor) => {
            println!("Goto Doctor Report:");
            let ws = storage.list_workspaces()?;
            println!("Workspaces: {}", ws.len());
            for w in ws {
                println!("  - {}", w.path.display());
            }
            let dirs = storage.list_directories()?;
            println!("Indexed directories: {}", dirs.len());
        }
        Some(Commands::Register { path }) => {
            let abs_path = if path.is_absolute() {
                path
            } else {
                std::env::current_dir()?.join(path)
            };

            if !abs_path.exists() || !abs_path.is_dir() {
                return Ok(());
            }

            // Fast path for shell hooks: upsert only current directory to avoid expensive recursive scans.
            core::index::upsert_directory(&storage, &abs_path)?;
            let canonical_abs_path =
                std::fs::canonicalize(&abs_path).unwrap_or_else(|_| abs_path.clone());

            // Find dir ID for visit recording
            let dirs = storage.list_directories()?;
            if let Some(dir) = dirs
                .iter()
                .find(|d| d.path == abs_path || d.path == canonical_abs_path)
            {
                storage.add_visit(VisitEvent {
                    path_id: dir.id,
                    timestamp: Utc::now(),
                })?;
            }
        }
        None => {
            let query = cli.query.join(" ");

            let selected_path = if let Some(direct_path) = resolve_direct_directory_query(&query)? {
                Some(direct_path.to_string_lossy().into_owned())
            } else if cli.auto {
                let results = core::search::search(&storage, &query)?;
                let mut found = None;
                for res in results {
                    if res.directory.path.exists() {
                        found = Some(res.directory.path.to_string_lossy().into_owned());
                        break;
                    } else {
                        // Dead path found, remove it and its subdirs from DB
                        let dead_path = res.directory.path.clone();
                        let dirs = storage.list_directories()?;
                        let to_remove: Vec<u64> = dirs
                            .iter()
                            .filter(|d| d.path.starts_with(&dead_path))
                            .map(|d| d.id)
                            .collect();
                        for id in to_remove {
                            let _ = storage.remove_directory(id);
                        }
                    }
                }
                found
            } else {
                ui::inline::run_ui(&storage, query.clone())?
            };

            if let Some(path) = selected_path {
                let path_buf = std::path::PathBuf::from(&path);
                if !path_buf.exists() {
                    // This could happen if it was deleted after run_ui selected it
                    eprintln!(
                        "Error: Directory '{}' no longer exists. Cleaning up index...",
                        path
                    );
                    // Cleanup this and all sub-directories from DB
                    let dirs = storage.list_directories()?;
                    let to_remove: Vec<u64> = dirs
                        .iter()
                        .filter(|d| d.path.starts_with(&path_buf))
                        .map(|d| d.id)
                        .collect();
                    for id in to_remove {
                        let _ = storage.remove_directory(id);
                    }
                    std::process::exit(1);
                }

                // Keep selection flow snappy by upserting only the selected directory.
                core::index::upsert_directory(&storage, &path_buf)?;

                // Find dir for path to record visit
                let dirs = storage.list_directories()?;
                if let Some(dir) = dirs.into_iter().find(|d| d.path == path_buf) {
                    storage.add_visit(VisitEvent {
                        path_id: dir.id,
                        timestamp: Utc::now(),
                    })?;
                    storage.update_query_mapping(&query, dir.id)?;
                }
                println!("{}", path);
            } else if cli.auto {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_direct_path_query, resolve_input_path_from_base};
    use std::path::{Path, PathBuf};

    #[test]
    fn detects_relative_and_absolute_path_queries() {
        assert!(is_direct_path_query("./abc/x"));
        assert!(is_direct_path_query("..//abc/y"));
        assert!(is_direct_path_query("/tmp"));
        assert!(is_direct_path_query("."));
        assert!(is_direct_path_query(".."));
        assert!(!is_direct_path_query("my-project"));
        assert!(!is_direct_path_query("@infra"));
    }

    #[test]
    fn resolves_parent_traversal_to_root_for_workspace_paths() {
        let base = Path::new("/home/abc");
        let resolved = resolve_input_path_from_base(base, Path::new("../../"));
        assert_eq!(resolved, PathBuf::from("/"));
    }
}
