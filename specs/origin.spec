CODE TASK — Implement goto (Rust Intelligent Directory Navigator)

You are implementing a Rust CLI tool named goto (alias g) for developers to quickly navigate to project directories using intelligent search, learned behavior, and workspace indexing.

The tool must integrate with the shell to change directories.

Primary goals:
	•	Extremely fast navigation across thousands of folders
	•	Intelligent ranking using history + fuzzy match
	•	Inline autocomplete UI (NOT fullscreen)
	•	Deterministic non-interactive mode for scripting
	•	Persistent storage using BadgerDB
	•	Cross-machine history via shared filesystem (no sync logic needed)

⸻

High-Level Behavior

Example usage:

g pay api

User receives inline suggestions below the cursor.

Press Enter → tool outputs selected path → shell wrapper performs cd.

⸻

Core Requirements

1. Language & Libraries

Use:
	•	Rust stable
	•	clap — CLI
	•	ratatui + crossterm — inline UI
	•	ignore or walkdir — filesystem scanning
	•	serde — serialization
	•	fuzzy-matcher or nucleo — fuzzy matching
	•	BadgerDB Rust binding — persistent storage

The UI must use ratatui inline viewport (not alternate fullscreen). Ratatui supports inline rendering via viewport configuration.  ￼

⸻

2. Binary & Alias

Binary:

goto

Expected shell alias:

g() {
  local dir
  dir="$(goto "$@")" && cd "$dir"
}

The program itself only prints the path.

⸻

3. Workspace System

Users configure workspace roots:

goto workspace add ~/code
goto workspace remove ~/code
goto workspace list

Behavior:
	•	Recursively index all directories
	•	Cache results in DB
	•	Ignore:
	•	.git
	•	node_modules
	•	target
	•	build
	•	configurable ignores

Support thousands of directories efficiently.

⸻

4. Indexing Command

goto index

Behavior:
	•	Scan all workspace roots
	•	Detect new / deleted directories
	•	Update metadata
	•	Incremental if possible

Metadata to store:

Directory {
    id
    path
    name
    depth
    last_seen
    project_type
}


⸻

5. Project Detection Heuristics

Detect type using markers:
	•	.git
	•	Cargo.toml
	•	package.json
	•	pyproject.toml
	•	Dockerfile

Store project_type enum.

This influences ranking.

⸻

6. Search System

Query features:
	•	Fuzzy match
	•	Substring match anywhere in path
	•	Multi-token matching
	•	Typo tolerance
	•	Middle-of-path matching
	•	Tag match (@infra)
	•	Learned mapping (query → directory)

⸻

7. Ranking Algorithm (Weighted Hybrid)

Combine:
	•	fuzzy_score
	•	recency_score
	•	frequency_score
	•	learned_mapping_score
	•	project_bonus

Example formula:

score =
    0.35 * fuzzy +
    0.25 * recency +
    0.20 * frequency +
    0.15 * learned +
    0.05 * project

Sorting fallback:
	1.	Score
	2.	Last used
	3.	Alphabetical

⸻

8. History & Learning

Store:

VisitEvent {
    path_id
    timestamp
}

QueryMapping {
    query
    path_id
    count
}

History file lives on shared filesystem (NFS). Do NOT implement sync logic.

⸻

9. Interactive Mode (Default)

When user runs:

goto <query>

Launch inline selector UI.

UI Requirements:
	•	Suggestions appear below cursor line
	•	Real-time filtering
	•	Keyboard:
	•	Arrow Up/Down
	•	Ctrl-J / Ctrl-K
	•	Enter → select
	•	Esc → cancel
	•	Show:
	•	directory name
	•	path
	•	project type icon
	•	score hint (optional)

Implementation Notes:
	•	Do NOT use alternate screen
	•	Render in inline viewport
	•	Restore cursor position after exit
	•	Handle terminal resize

Ratatui applications typically:
	1.	Initialize terminal
	2.	Run event loop
	3.	Draw frames
	4.	Restore terminal state  ￼

⸻

10. Non-Interactive Mode (“Feeling Lucky”)

Flag:

goto <query> --auto

Behavior:
	•	Choose highest ranked result
	•	Never prompt
	•	Deterministic
	•	Exit non-zero if no match

Used for scripts:

cd "$(goto api --auto)"


⸻

11. Tags System

Commands:

goto tag add <tag> <path>
goto tag remove <tag> <path>
goto tag list

Search:

g @infra

Tags influence ranking.

⸻

12. Storage Layer (BadgerDB)

Collections:
	•	directories
	•	visits
	•	query_mappings
	•	tags
	•	workspaces
	•	config

Design for:
	•	Fast lookup
	•	Low latency
	•	Concurrent safe reads

⸻

13. Performance Targets
	•	< 50ms search latency with thousands of folders
	•	Lazy load heavy metadata
	•	Cache hot ranking data in memory

⸻

14. CLI Commands Summary

goto <query>
goto <query> --auto

goto workspace add <path>
goto workspace remove <path>
goto workspace list

goto index

goto tag add <tag> <path>
goto tag remove <tag> <path>
goto tag list

goto doctor


⸻

Architecture

Suggested modules:

src/
  main.rs
  cli/
  core/
    search.rs
    ranking.rs
    index.rs
    history.rs
    tags.rs
    workspace.rs
    project_detect.rs
  storage/
    db.rs
    models.rs
  ui/
    inline.rs
    input.rs
  shell/
    output.rs


⸻

Inline UI Implementation Guidance

Use:
	•	Crossterm raw mode
	•	Ratatui Terminal with inline viewport
	•	Event loop polling keyboard events
	•	Render suggestion list widget

Avoid fullscreen alternate screen.

⸻

MVP Milestones

v0.1
	•	Workspace add/remove
	•	Index scan
	•	Basic fuzzy search
	•	Non-interactive mode
	•	Shell integration

v0.2
	•	Inline UI
	•	History tracking
	•	Ranking improvements

v0.3
	•	Tags
	•	Learned mappings
	•	Project detection

v1.0
	•	Performance optimization
	•	Config file
	•	Polished UX

⸻

Acceptance Criteria

The implementation is complete when:
	•	User can index workspace
	•	User can run g something and select interactively
	•	Enter changes directory via shell wrapper
	•	History affects ranking
	•	--auto mode works deterministically
	•	Thousands of directories remain fast

⸻

Nice-to-Have (Optional)
	•	Icons for project types
	•	Preview panel
	•	Git branch display
	•	Configurable weights

⸻

Deliverables
	1.	Compilable Rust project
	2.	README with install instructions
	3.	Example shell integration
	4.	Basic tests for ranking and search

⸻

Questions For Future Iterations (Do Not Block MVP)
	•	Git remote similarity ranking?
	•	Alias shortcuts?
	•	Background daemon?
	•	Telemetry?

⸻

If anything is unclear, choose reasonable defaults and proceed.
