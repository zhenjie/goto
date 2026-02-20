mod cli;
mod core;
mod storage;
mod ui;
mod shell;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, WorkspaceAction, TagAction};
use storage::db::Storage;
use storage::models::VisitEvent;
use chrono::Utc;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let storage = Storage::new()?;

    match cli.command {
        Some(Commands::Workspace { action }) => match action {
            WorkspaceAction::Add { path } => {
                let abs_path = std::fs::canonicalize(&path)
                    .unwrap_or_else(|_| if path.is_absolute() { path } else { std::env::current_dir().unwrap().join(path) });
                core::workspace::add_workspace(&storage, abs_path)?;
                println!("Workspace added. Indexing...");
                core::index::index_workspaces(&storage)?;
            }
            WorkspaceAction::Remove { path } => {
                let abs_path = std::fs::canonicalize(&path)
                    .unwrap_or_else(|_| if path.is_absolute() { path } else { std::env::current_dir().unwrap().join(path) });
                core::workspace::remove_workspace(&storage, abs_path.clone())?;
                println!("Workspace removed. Cleaning up index...");
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
                let abs_path = std::fs::canonicalize(&path)
                    .unwrap_or_else(|_| if path.is_absolute() { path } else { std::env::current_dir().unwrap().join(path) });
                // Find path ID
                let dirs = storage.list_directories()?;
                if let Some(dir) = dirs.into_iter().find(|d| d.path == abs_path) {
                    storage.add_tag(tag, dir.id)?;
                } else {
                    anyhow::bail!("Path not found in index. Run 'goto index' first.");
                }
            }
            TagAction::Remove { tag, path } => {
                let abs_path = std::fs::canonicalize(&path)
                    .unwrap_or_else(|_| if path.is_absolute() { path } else { std::env::current_dir().unwrap().join(path) });
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

            let dirs = storage.list_directories()?;
            let dir_id = if let Some(dir) = dirs.iter().find(|d| d.path == abs_path) {
                dir.id
            } else {
                let id = storage.next_directory_id()?;
                let name = abs_path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let depth = abs_path.components().count();
                let project_type = core::project_detect::detect_project_type(&abs_path);
                
                let new_dir = storage::models::Directory {
                    id,
                    path: abs_path.clone(),
                    name,
                    depth,
                    last_seen: Utc::now(),
                    project_type,
                };
                storage.add_directory(&new_dir)?;
                id
            };

            storage.add_visit(VisitEvent {
                path_id: dir_id,
                timestamp: Utc::now(),
            })?;
        }
        None => {
            let query = cli.query.join(" ");
            
            let selected_path = if cli.auto {
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
                        let to_remove: Vec<u64> = dirs.iter()
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
                    eprintln!("Error: Directory '{}' no longer exists. Cleaning up index...", path);
                    // Cleanup this and all sub-directories from DB
                    let dirs = storage.list_directories()?;
                    let to_remove: Vec<u64> = dirs.iter()
                        .filter(|d| d.path.starts_with(&path_buf))
                        .map(|d| d.id)
                        .collect();
                    for id in to_remove {
                        let _ = storage.remove_directory(id);
                    }
                    std::process::exit(1);
                }
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
            } else {
                if cli.auto {
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
