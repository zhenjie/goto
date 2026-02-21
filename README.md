# goto (g)

Intelligent Directory Navigator in Rust.

## Features
- Fuzzy search across workspace roots
- Intelligent ranking (recency, frequency, project type)
- Inline UI for selection
- Deterministic --auto mode for scripting
- Tagging system

## Installation
```bash
cargo install --path .
```

## Shell Integration
Add this to your shell config (e.g., `~/.bashrc` or `~/.zshrc`):

### Zsh / Bash
```bash
g() {
  local dir
  if [ "$#" -eq 1 ] && [ "$1" = "-" ]; then
    cd - || return
    return
  fi
  # If no args, just run interactive
  if [ $# -eq 0 ]; then
    dir="$(goto)"
  else
    # Try auto mode first. If it fails (no match), run interactive.
    # We use a temp variable to ensure we don't cd to empty strings.
    dir="$(goto "$@" --auto 2>/dev/null)" || dir="$(goto "$@")"
  fi
  
  if [ -n "$dir" ]; then
    cd "$dir"
  fi
}

gi() {
  local dir
  dir="$(goto "$@")"
  if [ -n "$dir" ]; then
    cd "$dir"
  fi
}
```

## Usage
### Setup Workspaces
```bash
goto workspace add ~/code
goto index
```

### Search and Navigate
```bash
g my-project
gi my-project
```

### Tags
```bash
goto tag add infra ~/code/infrastructure
g @infra
```

### Non-interactive
```bash
cd "$(goto my-project --auto)"
```
