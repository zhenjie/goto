use crate::core::project_detect::detect_project_type;
use crate::storage::db::Storage;
use crate::storage::models::Directory;
use anyhow::Result;
use chrono::Utc;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Default, Clone, Copy)]
pub struct IndexStats {
    pub workspaces: usize,
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

pub enum WorkspaceIndexEvent<'a> {
    Start {
        index: usize,
        total: usize,
        path: &'a Path,
    },
    Progress {
        index: usize,
        total: usize,
        path: &'a Path,
        scanned: usize,
        added: usize,
        updated: usize,
    },
    Complete {
        index: usize,
        total: usize,
        path: &'a Path,
        scanned: usize,
        added: usize,
        updated: usize,
    },
}

pub fn index_workspaces_with_progress<F>(storage: &Storage, mut on_event: F) -> Result<IndexStats>
where
    F: FnMut(WorkspaceIndexEvent<'_>),
{
    let ignore_config = crate::core::config::load_ignore_config()?;
    let workspaces = storage.list_workspaces()?;
    let total_workspaces = workspaces.len();
    let mut existing_dirs: std::collections::HashMap<std::path::PathBuf, Directory> = storage
        .list_directories()?
        .into_iter()
        .map(|d| (d.path.clone(), d))
        .collect();

    let mut all_scanned_paths = std::collections::HashSet::new();
    let mut stats = IndexStats {
        workspaces: total_workspaces,
        ..Default::default()
    };

    for (idx, ws) in workspaces.iter().enumerate() {
        let workspace_index = idx + 1;
        let mut workspace_scanned = 0usize;
        let mut workspace_added = 0usize;
        let mut workspace_updated = 0usize;
        on_event(WorkspaceIndexEvent::Start {
            index: workspace_index,
            total: total_workspaces,
            path: &ws.path,
        });

        let mut workspace_progress = |scanned: usize, added: usize, updated: usize| {
            workspace_scanned = scanned;
            workspace_added = added;
            workspace_updated = updated;
            on_event(WorkspaceIndexEvent::Progress {
                index: workspace_index,
                total: total_workspaces,
                path: &ws.path,
                scanned,
                added,
                updated,
            });
        };

        index_directory_internal(
            storage,
            &ws.path,
            &ignore_config,
            &mut existing_dirs,
            &mut all_scanned_paths,
            &mut stats,
            &mut workspace_progress,
        )?;

        on_event(WorkspaceIndexEvent::Complete {
            index: workspace_index,
            total: total_workspaces,
            path: &ws.path,
            scanned: workspace_scanned,
            added: workspace_added,
            updated: workspace_updated,
        });
    }

    // Cleanup: remove directories that are within ANY workspace but were not scanned this time
    let to_remove: Vec<u64> = existing_dirs
        .values()
        .filter(|d| {
            let in_some_workspace = workspaces.iter().any(|ws| d.path.starts_with(&ws.path));
            in_some_workspace && !all_scanned_paths.contains(&d.path)
        })
        .map(|d| d.id)
        .collect();

    for id in &to_remove {
        storage.remove_directory(*id)?;
    }
    stats.removed = to_remove.len();

    Ok(stats)
}

pub fn remove_indexed_subdirs_with_progress<F>(
    storage: &Storage,
    root: &Path,
    mut on_progress: F,
) -> Result<usize>
where
    F: FnMut(usize, usize),
{
    let dirs = storage.list_directories()?;
    let to_remove: Vec<u64> = dirs
        .into_iter()
        .filter(|d| d.path.starts_with(root))
        .map(|d| d.id)
        .collect();
    let total = to_remove.len();

    for (idx, id) in to_remove.iter().enumerate() {
        storage.remove_directory(*id)?;
        on_progress(idx + 1, total);
    }

    Ok(total)
}

pub fn upsert_directory(storage: &Storage, path: &std::path::Path) -> Result<()> {
    let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing_dirs: std::collections::HashMap<std::path::PathBuf, Directory> = storage
        .list_directories()?
        .into_iter()
        .map(|d| (d.path.clone(), d))
        .collect();

    let project_type = detect_project_type(&canonical_path);

    if let Some(dir) = existing_dirs.get_mut(&canonical_path) {
        dir.last_seen = Utc::now();
        dir.project_type = project_type;
        storage.add_directory(dir)?;
    } else {
        let id = storage.next_directory_id()?;
        let name = canonical_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_string());
        let depth = canonical_path.components().count();

        let new_dir = Directory {
            id,
            path: canonical_path,
            name,
            depth,
            last_seen: Utc::now(),
            project_type,
        };
        storage.add_directory(&new_dir)?;
    }

    Ok(())
}

fn index_directory_internal(
    storage: &Storage,
    root_path: &std::path::Path,
    ignore_config: &crate::core::config::IgnoreConfig,
    existing_dirs: &mut std::collections::HashMap<std::path::PathBuf, Directory>,
    all_scanned_paths: &mut std::collections::HashSet<std::path::PathBuf>,
    stats: &mut IndexStats,
    on_progress: &mut dyn FnMut(usize, usize, usize),
) -> Result<()> {
    let walker = WalkDir::new(root_path).into_iter().filter_entry(|e| {
        if e.depth() == 0 {
            return true;
        }
        let name = e.file_name().to_string_lossy();
        !name.starts_with('.')
            && !ignore_config.matches_name(&name)
            && !ignore_config.matches_path_prefix(e.path())
    });

    let mut workspace_scanned = 0usize;
    let mut workspace_added = 0usize;
    let mut workspace_updated = 0usize;

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_dir() {
            let path = entry.path().to_path_buf();
            all_scanned_paths.insert(path.clone());
            stats.scanned += 1;
            workspace_scanned += 1;

            let project_type = detect_project_type(&path);

            if let Some(dir) = existing_dirs.get_mut(&path) {
                dir.last_seen = Utc::now();
                dir.project_type = project_type;
                storage.add_directory(dir)?;
                stats.updated += 1;
                workspace_updated += 1;
            } else {
                let id = storage.next_directory_id()?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let depth = path.components().count();

                let new_dir = Directory {
                    id,
                    path: path.clone(),
                    name,
                    depth,
                    last_seen: Utc::now(),
                    project_type,
                };
                storage.add_directory(&new_dir)?;
                existing_dirs.insert(path.clone(), new_dir);
                stats.added += 1;
                workspace_added += 1;
            }

            on_progress(workspace_scanned, workspace_added, workspace_updated);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        WorkspaceIndexEvent, index_workspaces_with_progress, remove_indexed_subdirs_with_progress,
        upsert_directory,
    };
    use crate::storage::db::Storage;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("goto-{prefix}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn upsert_directory_only_registers_target_directory() {
        let db_path = test_dir("db-upsert");
        let root = test_dir("tree-upsert");
        let nested = root.join("a").join("b");

        std::fs::create_dir_all(&nested).expect("creates nested test tree");

        let storage = Storage::new_at_path(db_path.clone()).expect("opens storage");
        upsert_directory(&storage, &root).expect("upserts root directory");

        let dirs = storage.list_directories().expect("lists directories");
        assert_eq!(
            dirs.len(),
            1,
            "upsert should not recursively index children"
        );
        assert_eq!(
            dirs[0].path,
            std::fs::canonicalize(&root).expect("canonical root path")
        );

        let _ = std::fs::remove_dir_all(db_path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn index_workspaces_recursively_indexes_subdirectories() {
        let db_path = test_dir("db-workspaces");
        let root = test_dir("tree-workspaces");
        let nested = root.join("one").join("two");

        std::fs::create_dir_all(&nested).expect("creates nested test tree");

        let storage = Storage::new_at_path(db_path.clone()).expect("opens storage");
        storage
            .add_workspace(root.clone())
            .expect("adds workspace root");

        let stats = index_workspaces_with_progress(&storage, |_| {})
            .expect("indexes all workspace directories");
        assert_eq!(stats.workspaces, 1);
        assert_eq!(stats.scanned, 3);
        assert_eq!(stats.added, 3);
        assert_eq!(stats.removed, 0);

        let dirs = storage.list_directories().expect("lists directories");
        let paths: std::collections::HashSet<PathBuf> = dirs.into_iter().map(|d| d.path).collect();

        assert!(paths.contains(&root), "workspace root should be indexed");
        assert!(
            paths.contains(&root.join("one")),
            "first-level subdirectory should be indexed"
        );
        assert!(
            paths.contains(&nested),
            "nested subdirectory should be indexed"
        );

        let _ = std::fs::remove_dir_all(db_path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn remove_with_progress_reports_incremental_counts() {
        let db_path = test_dir("db-remove-progress");
        let root = test_dir("tree-remove-progress");
        let nested = root.join("one").join("two");

        std::fs::create_dir_all(&nested).expect("creates nested test tree");

        let storage = Storage::new_at_path(db_path.clone()).expect("opens storage");
        storage
            .add_workspace(root.clone())
            .expect("adds workspace root");
        index_workspaces_with_progress(&storage, |_| {}).expect("indexes workspace directories");

        let mut callbacks = 0usize;
        let mut last_done = 0usize;
        let mut last_total = 0usize;
        let removed = remove_indexed_subdirs_with_progress(&storage, &root, |done, total| {
            callbacks += 1;
            last_done = done;
            last_total = total;
        })
        .expect("removes indexed subdirectories");

        assert_eq!(removed, 3);
        assert_eq!(callbacks, 3);
        assert_eq!(last_done, 3);
        assert_eq!(last_total, 3);

        let remaining = storage
            .list_directories()
            .expect("lists remaining directories");
        assert!(
            remaining.into_iter().all(|d| !d.path.starts_with(&root)),
            "all directories under removed workspace should be deleted from index"
        );

        let _ = std::fs::remove_dir_all(db_path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn emits_workspace_progress_events_during_indexing() {
        let db_path = test_dir("db-events");
        let root = test_dir("tree-events");
        let nested = root.join("one").join("two");
        std::fs::create_dir_all(&nested).expect("creates nested test tree");

        let storage = Storage::new_at_path(db_path.clone()).expect("opens storage");
        storage
            .add_workspace(root.clone())
            .expect("adds workspace root");

        let mut saw_start = false;
        let mut saw_progress = false;
        let mut saw_complete = false;
        let mut final_scanned = 0usize;

        let _ = index_workspaces_with_progress(&storage, |event| match event {
            WorkspaceIndexEvent::Start { .. } => saw_start = true,
            WorkspaceIndexEvent::Progress { scanned, .. } => {
                saw_progress = true;
                final_scanned = scanned;
            }
            WorkspaceIndexEvent::Complete { .. } => saw_complete = true,
        })
        .expect("indexes workspace directories");

        assert!(saw_start, "should emit workspace start event");
        assert!(saw_progress, "should emit workspace progress events");
        assert!(saw_complete, "should emit workspace completion event");
        assert_eq!(final_scanned, 3);

        let _ = std::fs::remove_dir_all(db_path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn index_skips_pycache_from_default_ignore_names() {
        let db_path = test_dir("db-skip-pycache");
        let root = test_dir("tree-skip-pycache");
        let pycache = root.join("__pycache__");
        let pycache_nested = pycache.join("nested");
        let src = root.join("src");

        std::fs::create_dir_all(&pycache_nested).expect("creates pycache tree");
        std::fs::create_dir_all(&src).expect("creates src dir");

        let storage = Storage::new_at_path(db_path.clone()).expect("opens storage");
        storage
            .add_workspace(root.clone())
            .expect("adds workspace root");
        index_workspaces_with_progress(&storage, |_| {}).expect("indexes workspace directories");

        let dirs = storage.list_directories().expect("lists directories");
        let paths: std::collections::HashSet<std::path::PathBuf> =
            dirs.into_iter().map(|d| d.path).collect();

        assert!(paths.contains(&root), "workspace root should be indexed");
        assert!(paths.contains(&src), "normal directory should be indexed");
        assert!(
            !paths.contains(&pycache),
            "__pycache__ directory should be skipped"
        );
        assert!(
            !paths.contains(&pycache_nested),
            "children under __pycache__ should be skipped"
        );

        let _ = std::fs::remove_dir_all(db_path);
        let _ = std::fs::remove_dir_all(root);
    }
}
