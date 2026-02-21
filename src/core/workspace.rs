use crate::storage::db::Storage;
use anyhow::Result;
use std::path::PathBuf;

pub fn add_workspace(storage: &Storage, path: PathBuf) -> Result<()> {
    storage.add_workspace(path)
}

pub fn remove_workspace(storage: &Storage, path: PathBuf) -> Result<()> {
    storage.remove_workspace(&path)
}

pub fn list_workspaces(storage: &Storage) -> Result<Vec<PathBuf>> {
    let ws = storage.list_workspaces()?;
    Ok(ws.into_iter().map(|w| w.path).collect())
}
