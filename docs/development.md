# Development

## Quality checks

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo package
```

`just check` runs formatting verification, Clippy, and tests.

## Test boundaries

Linux tests cover plist parsing, SMC value decoding, configuration, SQLite compatibility, CLI behavior, and adaptive layout selection.

Live IOKit and `ioreg` collection must be validated on macOS hardware.

Use `tests/fixtures/real_mac.txt` for deterministic `ioreg` parser tests.

## Release build

```bash
cargo build --release
```

The binary is written to `target/release/pwer`.

## Publishing

Register `narumiruna/pwer` and `.github/workflows/publish.yml` as a Trusted Publisher for the `pwer` crate on crates.io.

Pushing a `v*` tag runs package validation, requests a short-lived crates.io token through GitHub OIDC, and publishes the locked crate.
