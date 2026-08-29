# TUI Usage

Launch the dashboard with:

```bash
pwer
```

The first reading is requested immediately.

Later readings use the configured interval.

## Controls

| Key | Action |
| --- | --- |
| `Q` | Quit |
| `Esc` | Quit |
| `R` | Wake the collector for an immediate refresh |
| `C` | Clear all stored readings and reset the current display |

## Panels

The live panel shows current power, negotiated charger power, battery capacity, voltage, current, charging state, charger metadata, and collection time.

The statistics panel shows average, minimum, and maximum power plus average battery percentage for the configured recent window.

The chart plots actual and negotiated power from oldest to newest within the configured chart window.

## Power signs

Positive voltage-times-current values indicate charging.

Negative values indicate battery discharge.

An available `PDTR` SMC reading is used directly and may follow the sensor convention of the current Mac model.

## Errors

Collection failures remain visible in the footer and do not terminate the dashboard.

Database write failures do not discard the current live display.

Press `R` after a transient failure to request another reading.
