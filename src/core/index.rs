use anyhow::Result;
use walkdir::WalkDir;
use chrono::Utc;
use crate::storage::db::Storage;
use crate::storage::models::Directory;
use crate::core::project_detect::detect_project_type;

pub fn index_workspaces(storage: &Storage) -> Result<()> {
    let workspaces = storage.list_workspaces()?;
    let mut existing_dirs: std::collections::HashMap<std::path::PathBuf, Directory> = 
        storage.list_directories()?.into_iter().map(|d| (d.path.clone(), d)).collect();

    let mut all_scanned_paths = std::collections::HashSet::new();

    for ws in &workspaces {
        let walker = WalkDir::new(&ws.path)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 0 { return true; }
                let name = e.file_name().to_string_lossy();
                name != ".git" && name != "node_modules" && name != "target" && name != "build" && !name.starts_with('.')
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
                    let name = path.file_name()
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
    }

    // Cleanup: remove directories that are within ANY workspace but were not scanned this time
    let to_remove: Vec<u64> = existing_dirs.values()
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
