mod cli;
mod core;
mod shell;
mod storage;
mod ui;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use cli::{Cli, Commands, TagAction, WorkspaceAction};
use core::index::WorkspaceIndexEvent;
use std::io::{self, Write};
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

fn is_confirmation_accepted(input: &str) -> bool {
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn confirm_purge(force: bool) -> Result<bool> {
    if force {
        return Ok(true);
    }

    eprintln!("WARNING: This will permanently remove all goto data, including workspaces.");
    eprint!("Type 'yes' to continue: ");
    io::stderr().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(is_confirmation_accepted(&answer))
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
                let mut last_printed = 0usize;
                let mut line_dirty = false;
                let stats = core::index::index_workspaces_with_progress(&storage, |event| {
                    match event {
                        WorkspaceIndexEvent::Start { index, total, path } => {
                            if line_dirty {
                                eprintln!();
                                line_dirty = false;
                            }
                            last_printed = 0;
                            eprintln!("Indexing workspace [{index}/{total}]: {}", path.display());
                        }
                        WorkspaceIndexEvent::Progress {
                            index,
                            total,
                            path,
                            scanned,
                            added,
                            updated,
                        } => {
                            if scanned == 1
                                || scanned % 500 == 0
                                || scanned.saturating_sub(last_printed) >= 500
                            {
                                eprint!(
                                    "\rIndexing workspace [{index}/{total}]: {} | scanned {scanned}, added {added}, updated {updated}",
                                    path.display()
                                );
                                let _ = io::stderr().flush();
                                last_printed = scanned;
                                line_dirty = true;
                            }
                        }
                        WorkspaceIndexEvent::Complete {
                            index,
                            total,
                            path,
                            scanned,
                            added,
                            updated,
                        } => {
                            eprintln!(
                                "\rIndexing workspace [{index}/{total}]: {} | scanned {scanned}, added {added}, updated {updated}",
                                path.display()
                            );
                            line_dirty = false;
                        }
                    }
                })?;
                if line_dirty {
                    eprintln!();
                }
                eprintln!(
                    "Index complete: scanned {}, added {}, updated {}, removed {} directories.",
                    stats.scanned, stats.added, stats.updated, stats.removed
                );
            }
            WorkspaceAction::Remove { path } => {
                let abs_path = resolve_input_path(&path)?;
                core::workspace::remove_workspace(&storage, abs_path.clone())?;
                eprintln!("Workspace removed. Cleaning up index...");
                let mut last_printed = 0usize;
                let removed = core::index::remove_indexed_subdirs_with_progress(
                    &storage,
                    &abs_path,
                    |done, total| {
                        if total == 0 {
                            return;
                        }
                        if done == 1 || done == total || done.saturating_sub(last_printed) >= 500 {
                            eprint!("\rCleaning up index... {done}/{total}");
                            let _ = io::stderr().flush();
                            last_printed = done;
                        }
                    },
                )?;
                if removed > 0 {
                    eprintln!("\rCleanup complete: removed {removed} directories.");
                } else {
                    eprintln!("Cleanup complete: removed 0 directories.");
                }
            }
            WorkspaceAction::List => {
                for path in core::workspace::list_workspaces(&storage)? {
                    println!("{}", path.display());
                }
            }
        },
        Some(Commands::Index) => {
            let mut last_printed = 0usize;
            let mut line_dirty = false;
            let stats = core::index::index_workspaces_with_progress(
                &storage,
                |event| match event {
                    WorkspaceIndexEvent::Start { index, total, path } => {
                        if line_dirty {
                            eprintln!();
                            line_dirty = false;
                        }
                        last_printed = 0;
                        eprintln!("Indexing workspace [{index}/{total}]: {}", path.display());
                    }
                    WorkspaceIndexEvent::Progress {
                        index,
                        total,
                        path,
                        scanned,
                        added,
                        updated,
                    } => {
                        if scanned == 1
                            || scanned % 500 == 0
                            || scanned.saturating_sub(last_printed) >= 500
                        {
                            eprint!(
                                "\rIndexing workspace [{index}/{total}]: {} | scanned {scanned}, added {added}, updated {updated}",
                                path.display()
                            );
                            let _ = io::stderr().flush();
                            last_printed = scanned;
                            line_dirty = true;
                        }
                    }
                    WorkspaceIndexEvent::Complete {
                        index,
                        total,
                        path,
                        scanned,
                        added,
                        updated,
                    } => {
                        eprintln!(
                            "\rIndexing workspace [{index}/{total}]: {} | scanned {scanned}, added {added}, updated {updated}",
                            path.display()
                        );
                        line_dirty = false;
                    }
                },
            )?;
            if line_dirty {
                eprintln!();
            }
            eprintln!(
                "Index complete: scanned {}, added {}, updated {}, removed {} directories across {} workspace(s).",
                stats.scanned, stats.added, stats.updated, stats.removed, stats.workspaces
            );
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
        Some(Commands::Purge { force }) => {
            if !confirm_purge(force)? {
                eprintln!("Purge cancelled.");
                return Ok(());
            }

            let stats = storage.purge_all()?;
            eprintln!(
                "Purge complete: removed {} directories, {} visits, {} query mappings, {} tags, {} workspaces.",
                stats.directories, stats.visits, stats.query_mappings, stats.tags, stats.workspaces
            );
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
    use super::{is_confirmation_accepted, is_direct_path_query, resolve_input_path_from_base};
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

    #[test]
    fn purge_confirmation_accepts_yes_and_y() {
        assert!(is_confirmation_accepted("yes"));
        assert!(is_confirmation_accepted("Y"));
        assert!(is_confirmation_accepted("  Yes  "));
        assert!(!is_confirmation_accepted("no"));
    }
}
