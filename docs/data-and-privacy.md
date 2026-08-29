# Data and Privacy

`pwer` stores data locally and sends no telemetry.

## Default files

- `~/.pwer/pwer.db` stores readings.
- `~/.pwer/config.toml` stores optional user configuration.
- `~/.pwer/pwer*.log` stores rotating application logs.

## Stored reading fields

The database stores timestamp, actual and negotiated watts, voltage, current, current and maximum capacity, battery percentage, charging flags, and optional charger name and manufacturer.

## Retention

```bash
pwer cleanup --days 30
```

This removes rows older than 30 days.

```bash
pwer cleanup --all
```

This clears every reading after confirmation.

Exports are ordinary local files and may include charger metadata.

Review exported files before sharing them.
