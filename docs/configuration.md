# Configuration

The optional configuration file is `~/.pwer/config.toml`.

Create it with:

```bash
pwer config init
```

## Format

```toml
[tui]
interval = 1.0
stats_limit = 100
chart_limit = 60

[database]
path = "~/.pwer/pwer.db"

[cli]
default_history_limit = 20
default_export_limit = 1000

[logging]
level = "INFO"
```

`interval` must be a positive number.

Every limit must be a positive integer.

`level` accepts `DEBUG`, `INFO`, `WARNING`, or `ERROR` without case sensitivity.

A leading `~/` in the database path expands to the current home directory.

## Precedence

1. CLI options override dashboard values.
2. Config file fields override built-in defaults.
3. Missing or invalid runtime fields fall back to their built-in defaults.

## Validation behavior

Normal startup is forgiving so one bad field does not prevent data access.

`pwer config validate` is strict and exits unsuccessfully for invalid TOML, unknown sections, unknown keys, malformed sections, invalid types, and invalid ranges.

`PWER_CONFIG` can point to another config file for automation and isolated testing.
