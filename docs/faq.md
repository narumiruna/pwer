# FAQ

## Does pwer support Linux or Windows?

Live collection does not, because it depends on macOS IOKit and `ioreg`.

Database and export commands can still operate on compatible SQLite files where the binary builds.

## Does it support Apple Silicon and Intel Macs?

The IOKit interface is architecture-independent, but available SMC keys vary by model.

Missing SMC sensors fall back to `ioreg` power calculation.

## Does it require root?

Normal use does not require root.

macOS may restrict individual SMC values depending on hardware and system policy.

## Where is history stored?

The default path is `~/.pwer/pwer.db`.

Change `[database].path` in the config file to use another location.

## Can it read data from the Python version?

Yes.

The Rust port preserves the table schema and accepts both legacy Peewee timestamps and new RFC 3339 timestamps.

## Does it send telemetry?

No.

The application performs local system calls, local file access, and local SQLite operations only.
