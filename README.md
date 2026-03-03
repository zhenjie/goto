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

## Database Path
By default, `goto` stores its database at:

- `$XDG_DATA_HOME/goto/db` when `XDG_DATA_HOME` is set
- `~/.local/share/goto/db` otherwise (Linux default)

You can override this path with:

```bash
export GOTO_DB_PATH=/path/to/custom/goto-db
```

## Shell Integration
Add this to your shell config (e.g., `~/.bashrc` or `~/.zshrc`):

### Zsh / Bash
```bash
# Register current directory after successful directory changes.
__goto_register_pwd() {
  command goto register "$PWD" >/dev/null 2>&1 || true
}

# Hook builtins so plain cd/pushd/popd also update goto history/index.
cd() {
  builtin cd "$@" || return
  __goto_register_pwd
}

pushd() {
  builtin pushd "$@" || return
  __goto_register_pwd
}

popd() {
  builtin popd "$@" || return
  __goto_register_pwd
}

# cd integration
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

# always-interactive cd
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

`goto workspace add` enforces ignore rules by default. Use `-f/--force` to bypass:

```bash
goto workspace add -f ~/some/ignored/path
```

Ignore rules are loaded from `~/.config/goto/config.toml`:

```toml
[ignore]
# Optional, defaults to true
use_defaults = true

# Ignore by final directory name
names = ["vendor", "third_party"]

# Ignore by absolute path prefix (supports ~/...)
paths = ["~/Downloads", "/mnt/huge-monorepo"]
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
