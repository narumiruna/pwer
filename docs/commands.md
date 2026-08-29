# CLI Commands

`pwer` launches the real-time dashboard without a subcommand.

## Export

```bash
pwer export readings.csv
pwer export readings.json --limit 1000
pwer export backup.txt --format csv
```

The format is inferred from `.csv` or `.json` unless `--format` is provided.

The default limit comes from `cli.default_export_limit`.

## Statistics

```bash
pwer stats
```

The command prints the total count, earliest and latest timestamps, database size, and database path.

## History

```bash
pwer history
pwer history --limit 50
```

Rows are selected newest first and displayed oldest first within the requested window.

## Battery health

```bash
pwer health
pwer health --days 60
```

The command groups maximum capacity by UTC date and compares the first and last daily averages.

Changes below `-0.5%` are labeled normal wear, and changes below `-2%` are labeled significant degradation.

## Cleanup

```bash
pwer cleanup --days 30
pwer cleanup --all
```

`--days` removes rows older than the requested retention period.

`--all` requires an interactive confirmation.

## Configuration

```bash
pwer config init
pwer config show
pwer config validate
```

`init` never overwrites an existing config file.

`show` prints the effective runtime values.

`validate` treats unknown structure and invalid values as errors.
