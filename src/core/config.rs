use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::Deserialize;
use std::path::{Path, PathBuf};

const DEFAULT_IGNORE_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "build",
    "dist",
    ".cache",
    ".venv",
    "venv",
    "__pycache__",
];
const DEFAULT_IGNORE_PATHS: &[&str] = &[];

#[derive(Debug, Default)]
pub struct IgnoreConfig {
    names: Vec<String>,
    paths: Vec<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    ignore: IgnoreSection,
}

#[derive(Debug, Default, Deserialize)]
struct IgnoreSection {
    use_defaults: Option<bool>,
    names: Option<Vec<String>>,
    paths: Option<Vec<String>>,
}

impl IgnoreConfig {
    pub fn is_ignored(&self, path: &Path) -> bool {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        if let Some(name) = canonical.file_name() {
            let candidate = name.to_string_lossy();
            if self.matches_name(&candidate) {
                return true;
            }
        }

        self.matches_path_prefix(&canonical)
    }

    pub fn matches_name(&self, name: &str) -> bool {
        let candidate = name.to_lowercase();
        self.names.iter().any(|n| n == &candidate)
    }

    pub fn matches_path_prefix(&self, path: &Path) -> bool {
        self.paths.iter().any(|ignored| path.starts_with(ignored))
    }
}

pub fn load_ignore_config() -> Result<IgnoreConfig> {
    let base_dirs = BaseDirs::new().context("Could not determine home directory")?;
    let config_path = base_dirs.home_dir().join(".config/goto/config.toml");
    load_ignore_config_from_path(&config_path)
}

pub fn load_ignore_config_from_path(path: &Path) -> Result<IgnoreConfig> {
    let mut names: Vec<String> = DEFAULT_IGNORE_NAMES
        .iter()
        .map(|name| name.to_lowercase())
        .collect();
    let mut paths: Vec<PathBuf> = DEFAULT_IGNORE_PATHS.iter().map(PathBuf::from).collect();

    if !path.exists() {
        return Ok(IgnoreConfig { names, paths });
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file at {}", path.display()))?;
    let file_config: FileConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML config at {}", path.display()))?;

    let use_defaults = file_config.ignore.use_defaults.unwrap_or(true);
    if !use_defaults {
        names.clear();
        paths.clear();
    }

    if let Some(extra_names) = file_config.ignore.names {
        names.extend(extra_names.into_iter().map(|name| name.to_lowercase()));
    }

    if let Some(extra_paths) = file_config.ignore.paths {
        paths.extend(extra_paths.into_iter().map(|p| expand_home_path(&p)));
    }

    Ok(IgnoreConfig { names, paths })
}

fn expand_home_path(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(base_dirs) = BaseDirs::new()
    {
        return base_dirs.home_dir().join(rest);
    }

    PathBuf::from(input)
}

#[cfg(test)]
mod tests {
    use super::load_ignore_config_from_path;
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
    fn default_rules_ignore_common_large_or_generated_paths() {
        let missing = test_dir("missing-config").join("config.toml");
        let cfg = load_ignore_config_from_path(&missing).expect("loads default config");

        assert!(cfg.is_ignored(std::path::Path::new("/project/__pycache__")));
        assert!(cfg.is_ignored(std::path::Path::new("/project/target")));
        assert!(!cfg.is_ignored(std::path::Path::new("/tmp/something")));
        assert!(!cfg.is_ignored(std::path::Path::new("/project/src")));
    }

    #[test]
    fn file_rules_extend_default_ignores() {
        let root = test_dir("config-extend");
        std::fs::create_dir_all(&root).expect("creates test dir");
        let config_path = root.join("config.toml");
        std::fs::write(
            &config_path,
            "[ignore]\nnames = [\"vendor\"]\npaths = [\"/opt/big-monorepo\"]\n",
        )
        .expect("writes config file");

        let cfg = load_ignore_config_from_path(&config_path).expect("loads config");

        assert!(cfg.is_ignored(std::path::Path::new("/code/vendor")));
        assert!(cfg.is_ignored(std::path::Path::new("/opt/big-monorepo/app")));
        assert!(cfg.is_ignored(std::path::Path::new("/project/node_modules")));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_rules_can_disable_defaults() {
        let root = test_dir("config-disable-defaults");
        std::fs::create_dir_all(&root).expect("creates test dir");
        let config_path = root.join("config.toml");
        std::fs::write(
            &config_path,
            "[ignore]\nuse_defaults = false\npaths = [\"/var/tmp/custom\"]\n",
        )
        .expect("writes config file");

        let cfg = load_ignore_config_from_path(&config_path).expect("loads config");

        assert!(!cfg.is_ignored(std::path::Path::new("/project/node_modules")));
        assert!(cfg.is_ignored(std::path::Path::new("/var/tmp/custom/service")));

        let _ = std::fs::remove_dir_all(root);
    }
}
