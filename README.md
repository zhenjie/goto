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

### Zsh
```zsh
g() {
  local dir
  dir="$(goto "$@")" && [ -n "$dir" ] && cd "$dir"
}
chpwd() {
  goto register "$PWD"
}
# Register current directory on startup
goto register "$PWD"
```

### Bash
```bash
g() {
  local dir
  dir="$(goto "$@")" && [ -n "$dir" ] && cd "$dir"
}
# Record every cd/pushd/popd
cd() { builtin cd "$@" && goto register "$PWD"; }
pushd() { builtin pushd "$@" && goto register "$PWD"; }
popd() { builtin popd "$@" && goto register "$PWD"; }
# Register current directory on startup
goto register "$PWD"
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
