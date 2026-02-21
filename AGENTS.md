# Repository Guidelines

## Project Structure & Module Organization
This is a Rust CLI project (`goto`) organized by feature area under `src/`:
- `src/main.rs`: command dispatch and top-level flow.
- `src/cli/`: Clap argument and subcommand definitions.
- `src/core/`: indexing, matching, ranking, search, and workspace logic.
- `src/storage/`: persistence layer (`sled`) and data models.
- `src/ui/`: interactive inline terminal UI.
- `src/shell/`: shell integration helpers.
- `specs/origin.spec`: behavior/reference spec.
- `install.sh`: local build + install helper.

## Build, Test, and Development Commands
- `cargo build`: debug build for local development.
- `cargo build --release`: optimized binary in `target/release/goto`.
- `cargo run -- <args>`: run locally, example: `cargo run -- workspace list`.
- `cargo test`: run unit tests (including module-local tests).
- `cargo fmt`: format code with Rustfmt.
- `cargo clippy -- -D warnings`: lint and fail on warnings.
- `./install.sh`: build and install with shell function guidance.

## Coding Style & Naming Conventions
- Follow Rust 2024 idioms and keep code `rustfmt`-clean.
- Use 4-space indentation and snake_case for files, modules, and functions.
- Use `PascalCase` for structs/enums and `SCREAMING_SNAKE_CASE` for constants.
- Keep modules focused by domain (`core`, `storage`, `ui`) and avoid cross-layer leakage.
- Prefer explicit error propagation with `anyhow::Result` at boundaries.

## Testing Guidelines
- Use Rust’s built-in test framework with `#[cfg(test)]` and `#[test]`.
- Keep unit tests near implementation (see `src/core/matcher.rs`).
- Name tests by behavior, e.g. `test_case_insensitivity`, `test_ordered_matching`.
- Run `cargo test` before opening a PR; add tests for every bug fix and new matching/ranking rule.

## Commit & Pull Request Guidelines
- Current history is minimal (`Update`), so use clear imperative subjects going forward.
- Recommended commit format: `<area>: <imperative summary>` (example: `core: prune dead paths in auto mode`).
- Keep commits scoped and atomic; avoid mixing refactors with behavior changes.
- PRs should include what changed and why, user-visible impact (with CLI examples when behavior changes), linked issues when available, and test evidence (`cargo test`, `cargo clippy`).
