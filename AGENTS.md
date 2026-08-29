# Repository Guidelines

## Repository structure

- [UNREVIEWED] Keep CLI orchestration in `src/cli.rs`, configuration in `src/config.rs`, SQLite access in `src/database.rs`, and terminal rendering in `src/tui.rs`.
- [UNREVIEWED] Keep macOS collection under `src/collector/`, and keep IOKit FFI behind `cfg(target_os = "macos")` so Linux CI builds remain valid.
- [UNREVIEWED] Treat `tests/fixtures/real_mac.txt` as deterministic parser input and `target/` as generated output.

## Commands

- [UNREVIEWED] Run `cargo fmt --all --check` to verify formatting.
- [UNREVIEWED] Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce warning-free code.
- [UNREVIEWED] Run `cargo test --all-targets --all-features` for the complete automated test suite.
- [UNREVIEWED] Run `cargo package` before release changes.
- [UNREVIEWED] Use `cargo run -- COMMAND` to run the local CLI.

## Code style

- [UNREVIEWED] Follow `rustfmt` defaults and use `snake_case` for functions and variables and `CamelCase` for types.
- [UNREVIEWED] Keep `unsafe` code limited to the macOS IOKit boundary and document each unsafe operation's preconditions.
- [UNREVIEWED] Preserve deterministic query ordering and stable CSV and JSON field names.
- [UNREVIEWED] Preserve compatibility with the existing `power_readings` SQLite schema and legacy Python/Peewee timestamp strings.

## Testing

- [UNREVIEWED] Add unit tests beside parser, configuration, database, and layout code and CLI process tests under `tests/`.
- [UNREVIEWED] Do not require live battery state in automated tests when a fixture or temporary SQLite database can prove behavior.
- [UNREVIEWED] Treat live IOKit, SMC, and `ioreg` checks as macOS-only validation.

## Security

- [UNREVIEWED] Do not commit user databases, exported power logs, machine-specific config, or files from `~/.pwer/`.
- [UNREVIEWED] Preserve local-only operation and do not add telemetry or network access without explicit approval.
