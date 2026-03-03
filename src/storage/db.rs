use crate::storage::models::*;
use anyhow::{Context, Result};
use bincode;
use directories::ProjectDirs;
use sled::{Db, Tree};
use std::path::{Path, PathBuf};

pub struct Storage {
    db: Db,
    directories: Tree,
    visits: Tree,
    query_mappings: Tree,
    tags: Tree,
    workspaces: Tree,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PurgeStats {
    pub directories: usize,
    pub visits: usize,
    pub query_mappings: usize,
    pub tags: usize,
    pub workspaces: usize,
}

impl Storage {
    pub fn new() -> Result<Self> {
        let db_path = if let Ok(path) = std::env::var("GOTO_DB_PATH") {
            PathBuf::from(path)
        } else {
            let proj_dirs = ProjectDirs::from("com", "goto", "goto")
                .context("Could not determine project directories")?;
            let data_dir = proj_dirs.data_dir();
            std::fs::create_dir_all(data_dir)?;
            data_dir.join("db")
        };

        Self::new_at_path(db_path)
    }

    pub fn new_at_path(db_path: PathBuf) -> Result<Self> {
        let db = sled::open(db_path)?;

        Ok(Self {
            directories: db.open_tree("directories")?,
            visits: db.open_tree("visits")?,
            query_mappings: db.open_tree("query_mappings")?,
            tags: db.open_tree("tags")?,
            workspaces: db.open_tree("workspaces")?,
            db,
        })
    }

    // Directory operations
    pub fn add_directory(&self, dir: &Directory) -> Result<()> {
        let key = dir.id.to_be_bytes();
        let value = bincode::serialize(dir)?;
        self.directories.insert(key, value)?;
        Ok(())
    }

    pub fn get_directory(&self, id: u64) -> Result<Option<Directory>> {
        let key = id.to_be_bytes();
        if let Some(ivec) = self.directories.get(key)? {
            let dir: Directory = bincode::deserialize(&ivec)?;
            Ok(Some(dir))
        } else {
            Ok(None)
        }
    }

    pub fn remove_directory(&self, id: u64) -> Result<()> {
        let key = id.to_be_bytes();
        self.directories.remove(key)?;
        self.directories.flush()?;
        Ok(())
    }

    pub fn list_directories(&self) -> Result<Vec<Directory>> {
        let mut dirs = Vec::new();
        for item in self.directories.iter() {
            let (_, value) = item?;
            let dir: Directory = bincode::deserialize(&value)?;
            dirs.push(dir);
        }
        Ok(dirs)
    }

    pub fn next_directory_id(&self) -> Result<u64> {
        Ok(self.db.generate_id()?)
    }

    // Workspace operations
    pub fn add_workspace(&self, path: PathBuf) -> Result<()> {
        let key = path.to_string_lossy().as_bytes().to_vec();
        let workspace = Workspace { path };
        let value = bincode::serialize(&workspace)?;
        self.workspaces.insert(key, value)?;
        Ok(())
    }

    pub fn remove_workspace(&self, path: &Path) -> Result<()> {
        let key = path.to_string_lossy();
        self.workspaces.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let mut ws = Vec::new();
        for item in self.workspaces.iter() {
            let (_, value) = item?;
            let w: Workspace = bincode::deserialize(&value)?;
            ws.push(w);
        }
        Ok(ws)
    }

    // Visit operations
    pub fn add_visit(&self, event: VisitEvent) -> Result<()> {
        let key = self.db.generate_id()?.to_be_bytes();
        let value = bincode::serialize(&event)?;
        self.visits.insert(key, value)?;
        Ok(())
    }

    pub fn list_visits(&self) -> Result<Vec<VisitEvent>> {
        let mut visits = Vec::new();
        for item in self.visits.iter() {
            let (_, value) = item?;
            let v: VisitEvent = bincode::deserialize(&value)?;
            visits.push(v);
        }
        Ok(visits)
    }

    // Query mappings
    pub fn update_query_mapping(&self, query: &str, path_id: u64) -> Result<()> {
        let key = query.as_bytes();
        let mapping = if let Some(ivec) = self.query_mappings.get(key)? {
            let mut m: QueryMapping = bincode::deserialize(&ivec)?;
            if m.path_id == path_id {
                m.count += 1;
                m
            } else {
                QueryMapping {
                    query: query.to_string(),
                    path_id,
                    count: 1,
                }
            }
        } else {
            QueryMapping {
                query: query.to_string(),
                path_id,
                count: 1,
            }
        };

        let value = bincode::serialize(&mapping)?;
        self.query_mappings.insert(key, value)?;
        Ok(())
    }

    pub fn get_query_mappings(&self) -> Result<Vec<QueryMapping>> {
        let mut mappings = Vec::new();
        for item in self.query_mappings.iter() {
            let (_, value) = item?;
            let m: QueryMapping = bincode::deserialize(&value)?;
            mappings.push(m);
        }
        Ok(mappings)
    }

    // Tags
    pub fn add_tag(&self, name: String, path_id: u64) -> Result<()> {
        let key = format!("{}:{}", name, path_id).into_bytes();
        let tag = Tag { name, path_id };
        let value = bincode::serialize(&tag)?;
        self.tags.insert(key, value)?;
        Ok(())
    }

    pub fn remove_tag(&self, name: &str, path_id: u64) -> Result<()> {
        let key = format!("{}:{}", name, path_id).into_bytes();
        self.tags.remove(key)?;
        Ok(())
    }

    pub fn list_tags(&self) -> Result<Vec<Tag>> {
        let mut tags = Vec::new();
        for item in self.tags.iter() {
            let (_, value) = item?;
            let t: Tag = bincode::deserialize(&value)?;
            tags.push(t);
        }
        Ok(tags)
    }

    pub fn purge_all(&self) -> Result<PurgeStats> {
        let stats = PurgeStats {
            directories: self.directories.len(),
            visits: self.visits.len(),
            query_mappings: self.query_mappings.len(),
            tags: self.tags.len(),
            workspaces: self.workspaces.len(),
        };

        self.directories.clear()?;
        self.visits.clear()?;
        self.query_mappings.clear()?;
        self.tags.clear()?;
        self.workspaces.clear()?;
        self.db.flush()?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::Storage;
    use crate::storage::models::{Directory, ProjectType, VisitEvent};
    use chrono::Utc;
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
    fn purge_all_clears_all_trees() {
        let db_path = test_dir("db-purge");
        let storage = Storage::new_at_path(db_path.clone()).expect("opens storage");

        let root = test_dir("purge-workspace");
        std::fs::create_dir_all(&root).expect("creates workspace dir");
        storage
            .add_workspace(root.clone())
            .expect("adds workspace entry");

        let dir = Directory {
            id: storage.next_directory_id().expect("gets directory id"),
            path: root.clone(),
            name: "purge-workspace".to_string(),
            depth: root.components().count(),
            last_seen: Utc::now(),
            project_type: ProjectType::Unknown,
        };
        storage.add_directory(&dir).expect("adds directory");
        storage
            .add_visit(VisitEvent {
                path_id: dir.id,
                timestamp: Utc::now(),
            })
            .expect("adds visit");
        storage
            .update_query_mapping("purge-test", dir.id)
            .expect("adds mapping");
        storage
            .add_tag("tmp".to_string(), dir.id)
            .expect("adds tag");

        let stats = storage.purge_all().expect("purges database trees");
        assert!(stats.directories >= 1);
        assert!(stats.visits >= 1);
        assert!(stats.query_mappings >= 1);
        assert!(stats.tags >= 1);
        assert!(stats.workspaces >= 1);

        assert!(storage.list_directories().expect("lists dirs").is_empty());
        assert!(storage.list_visits().expect("lists visits").is_empty());
        assert!(
            storage
                .get_query_mappings()
                .expect("lists query mappings")
                .is_empty()
        );
        assert!(storage.list_tags().expect("lists tags").is_empty());
        assert!(storage.list_workspaces().expect("lists ws").is_empty());

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(db_path);
    }
}
