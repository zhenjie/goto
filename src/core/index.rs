use crate::core::project_detect::detect_project_type;
use crate::storage::db::Storage;
use crate::storage::models::Directory;
use anyhow::Result;
use chrono::Utc;
use walkdir::WalkDir;

pub fn index_workspaces(storage: &Storage) -> Result<()> {
    let workspaces = storage.list_workspaces()?;
    let mut existing_dirs: std::collections::HashMap<std::path::PathBuf, Directory> = storage
        .list_directories()?
        .into_iter()
        .map(|d| (d.path.clone(), d))
        .collect();

    let mut all_scanned_paths = std::collections::HashSet::new();

    for ws in &workspaces {
        index_directory_internal(
            storage,
            &ws.path,
            &mut existing_dirs,
            &mut all_scanned_paths,
        )?;
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

    for id in to_remove {
        storage.remove_directory(id)?;
    }
    Ok(())
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
    existing_dirs: &mut std::collections::HashMap<std::path::PathBuf, Directory>,
    all_scanned_paths: &mut std::collections::HashSet<std::path::PathBuf>,
) -> Result<()> {
    let walker = WalkDir::new(root_path).into_iter().filter_entry(|e| {
        if e.depth() == 0 {
            return true;
        }
        let name = e.file_name().to_string_lossy();
        name != ".git"
            && name != "node_modules"
            && name != "target"
            && name != "build"
            && !name.starts_with('.')
    });

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_dir() {
            let path = entry.path().to_path_buf();
            all_scanned_paths.insert(path.clone());

            let project_type = detect_project_type(&path);

            if let Some(dir) = existing_dirs.get_mut(&path) {
                dir.last_seen = Utc::now();
                dir.project_type = project_type;
                storage.add_directory(dir)?;
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
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{index_workspaces, upsert_directory};
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
        assert_eq!(dirs.len(), 1, "upsert should not recursively index children");
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

        index_workspaces(&storage).expect("indexes all workspace directories");

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
}
