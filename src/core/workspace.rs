use crate::storage::db::Storage;
use anyhow::Result;
use anyhow::bail;
use std::path::Path;
use std::path::PathBuf;

pub fn add_workspace(storage: &Storage, path: PathBuf, force: bool) -> Result<()> {
    if !force && path == Path::new("/") {
        bail!("Refusing to add '/' as a workspace without --force.");
    }

    if !force {
        let ignore_config = crate::core::config::load_ignore_config()?;
        if ignore_config.is_ignored(&path) {
            bail!(
                "Path '{}' is blocked by ignore rules. Use goto workspace add -f <path> to force add.",
                path.display()
            );
        }
    }

    storage.add_workspace(path)
}

pub fn remove_workspace(storage: &Storage, path: PathBuf) -> Result<()> {
    storage.remove_workspace(&path)
}

pub fn list_workspaces(storage: &Storage) -> Result<Vec<PathBuf>> {
    let ws = storage.list_workspaces()?;
    Ok(ws.into_iter().map(|w| w.path).collect())
}

#[cfg(test)]
mod tests {
    use super::add_workspace;
    use crate::storage::db::Storage;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("goto-{prefix}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn rejects_root_workspace_without_force() {
        let db_path = test_dir("db-root-guard");
        let storage = Storage::new_at_path(db_path.clone()).expect("opens storage");

        let result = add_workspace(&storage, Path::new("/").to_path_buf(), false);

        assert!(
            result.is_err(),
            "root workspace should be rejected without force"
        );

        let _ = std::fs::remove_dir_all(db_path);
    }
}
