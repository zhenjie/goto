use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "goto")]
#[command(about = "Intelligent Directory Navigator", long_about = None)]
pub struct Cli {
    #[arg(num_args = 0..)]
    pub query: Vec<String>,

    #[arg(short, long)]
    pub auto: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Workspace management
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },
    /// Index all workspaces
    Index,
    /// Tag management
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },
    /// Check system status
    Doctor,
    /// Purge all goto data (workspaces + index + history)
    Purge {
        /// Force purge without interactive confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Register a directory visit (called by shell hooks)
    Register { path: PathBuf },
}

#[derive(Subcommand)]
pub enum WorkspaceAction {
    /// Add a workspace root
    Add {
        /// Force add even if path matches ignore rules
        #[arg(short, long)]
        force: bool,
        path: PathBuf,
    },
    /// Remove a workspace root
    Remove { path: PathBuf },
    /// List all workspace roots
    List,
}

#[derive(Subcommand)]
pub enum TagAction {
    /// Add a tag to a path
    Add { tag: String, path: PathBuf },
    /// Remove a tag from a path
    Remove { tag: String, path: PathBuf },
    /// List all tags
    List,
}
