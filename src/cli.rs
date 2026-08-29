use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::collector::default_collector;
use crate::config::{PwerConfig, get_config_path, init_config, load_config, validate_config_file};
use crate::database::Database;
use crate::models::PowerReading;
use crate::tui::run_tui;

#[derive(Debug, Parser)]
#[command(
    name = "pwer",
    version,
    about = "macOS power monitoring tool with TUI and data export"
)]
pub struct Cli {
    #[arg(
        short,
        long,
        global = true,
        help = "Data collection interval in seconds (overrides config file)"
    )]
    interval: Option<f64>,
    #[arg(
        long,
        global = true,
        help = "Number of readings to include in statistics (overrides config file)"
    )]
    stats_limit: Option<usize>,
    #[arg(
        long,
        global = true,
        help = "Number of readings to display in chart (overrides config file)"
    )]
    chart_limit: Option<usize>,
    #[arg(
        long,
        global = true,
        help = "Enable debug logging (overrides config file)"
    )]
    debug: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Export power readings to CSV or JSON.
    Export(ExportArgs),
    /// Show database count, date range, size, and path.
    Stats,
    /// Delete old readings or clear all saved data.
    Cleanup(CleanupArgs),
    /// Show recent power readings.
    History(HistoryArgs),
    /// Analyze battery maximum-capacity trends.
    Health(HealthArgs),
    /// Create, inspect, or validate the config file.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Args)]
struct ExportArgs {
    /// Output file path.
    output: PathBuf,
    #[arg(short = 'n', long, help = "Maximum number of readings to export")]
    limit: Option<usize>,
    #[arg(
        short = 'f',
        long = "format",
        value_enum,
        help = "Output format; inferred from extension when omitted"
    )]
    format: Option<ExportFormat>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ExportFormat {
    Csv,
    Json,
}

#[derive(Debug, Args)]
struct CleanupArgs {
    #[arg(short, long, conflicts_with = "all")]
    days: Option<u64>,
    #[arg(
        long,
        conflicts_with = "days",
        help = "Delete all readings after confirmation"
    )]
    all: bool,
}

#[derive(Debug, Args)]
struct HistoryArgs {
    #[arg(short = 'n', long)]
    limit: Option<usize>,
}

#[derive(Debug, Args)]
struct HealthArgs {
    #[arg(short, long, default_value_t = 30)]
    days: u64,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Show the effective configuration.
    Show,
    /// Create a commented default config file.
    Init,
    /// Strictly validate the config file.
    Validate,
}

impl Cli {
    pub fn config(&self) -> Result<PwerConfig> {
        let base = load_config();
        PwerConfig::new(
            self.interval.unwrap_or(base.collection_interval),
            self.stats_limit.unwrap_or(base.stats_history_limit),
            self.chart_limit.unwrap_or(base.chart_history_limit),
            base.database_path,
            base.default_history_limit,
            base.default_export_limit,
            if self.debug {
                "DEBUG".into()
            } else {
                base.log_level
            },
        )
        .map_err(anyhow::Error::msg)
    }
}

pub fn run(cli: Cli, config: PwerConfig) -> Result<()> {
    match cli.command {
        None => launch_tui(config, cli.debug),
        Some(Command::Export(args)) => export(config, args),
        Some(Command::Stats) => stats(config),
        Some(Command::Cleanup(args)) => cleanup(config, args),
        Some(Command::History(args)) => history(config, args),
        Some(Command::Health(args)) => health(config, args),
        Some(Command::Config { command }) => config_command(command),
    }
}

fn launch_tui(config: PwerConfig, verbose: bool) -> Result<()> {
    let collector = default_collector(verbose)?;
    let database = Database::open(&config.database_path)?;
    run_tui(config, collector, database)
}

fn export(config: PwerConfig, args: ExportArgs) -> Result<()> {
    let format = match args.format {
        Some(format) => format,
        None => match args
            .output
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("csv") => ExportFormat::Csv,
            Some("json") => ExportFormat::Json,
            extension => anyhow::bail!(
                "Cannot detect format from extension '{}'. Use --format csv or --format json",
                extension.unwrap_or_default()
            ),
        },
    };
    let database = Database::open(&config.database_path)?;
    let readings =
        database.query_history(Some(args.limit.unwrap_or(config.default_export_limit)))?;
    if readings.is_empty() {
        println!("No readings found in database");
        return Ok(());
    }
    match format {
        ExportFormat::Csv => export_csv(&args.output, &readings)?,
        ExportFormat::Json => export_json(&args.output, &readings)?,
    }
    println!(
        "Exported {} readings to {}",
        readings.len(),
        args.output.display()
    );
    Ok(())
}

fn export_csv(path: &PathBuf, readings: &[PowerReading]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    for reading in readings {
        writer.serialize(reading)?;
    }
    writer.flush()?;
    Ok(())
}

fn export_json(path: &PathBuf, readings: &[PowerReading]) -> Result<()> {
    let file =
        fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    serde_json::to_writer_pretty(file, readings)?;
    Ok(())
}

fn stats(config: PwerConfig) -> Result<()> {
    if !config.database_path.exists() {
        println!("Database file does not exist yet");
        return Ok(());
    }
    let size = fs::metadata(&config.database_path)?.len() as f64 / (1024.0 * 1024.0);
    let database = Database::open(&config.database_path)?;
    let statistics = database.get_statistics(None)?;
    if statistics.count == 0 {
        println!("No readings in database");
        return Ok(());
    }
    println!("Database Statistics");
    println!("Total readings    {}", statistics.count);
    println!("Earliest reading  {}", format_date(statistics.earliest));
    println!("Latest reading    {}", format_date(statistics.latest));
    println!("Database size     {size:.2} MB");
    println!("Database path     {}", database.path().display());
    Ok(())
}

fn cleanup(config: PwerConfig, args: CleanupArgs) -> Result<()> {
    if args.days.is_none() && !args.all {
        anyhow::bail!("Must specify either --days N or --all");
    }
    if args.days == Some(0) {
        anyhow::bail!("--days must be a positive integer");
    }
    let database = Database::open(&config.database_path)?;
    if args.all {
        print!("WARNING: This will delete ALL readings. Continue? [y/N] ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Operation cancelled");
            return Ok(());
        }
        println!("Deleted all {} readings", database.clear_history()?);
    } else if let Some(days) = args.days {
        println!("Deleted {} old readings", database.cleanup_old_data(days)?);
    }
    Ok(())
}

fn history(config: PwerConfig, args: HistoryArgs) -> Result<()> {
    let database = Database::open(&config.database_path)?;
    let readings =
        database.query_history(Some(args.limit.unwrap_or(config.default_history_limit)))?;
    if readings.is_empty() {
        println!("No readings in database");
        return Ok(());
    }
    println!("Recent Power Readings (Last {})", readings.len());
    println!("Time      Power     Battery  Voltage  Current  Status");
    for reading in readings.iter().rev() {
        let status = if reading.is_charging {
            "Charging"
        } else if reading.external_connected {
            "AC Power"
        } else {
            "Battery"
        };
        println!(
            "{}  {:+7.1}W  {:>6}%  {:>6.1}V  {:+7.2}A  {}",
            reading.timestamp.format("%H:%M:%S"),
            reading.watts_actual,
            reading.battery_percent,
            reading.voltage,
            reading.amperage,
            status
        );
    }
    Ok(())
}

fn health(config: PwerConfig, args: HealthArgs) -> Result<()> {
    let database = Database::open(&config.database_path)?;
    let results = database.get_battery_health_trend(args.days)?;
    if results.is_empty() {
        println!("No readings found in the last {} days", args.days);
        return Ok(());
    }
    let first = &results[0];
    let last = results.last().expect("non-empty health trend");
    let change = last.average_max_capacity - first.average_max_capacity;
    let percent = if first.average_max_capacity == 0.0 {
        0.0
    } else {
        change / first.average_max_capacity * 100.0
    };
    let status = if percent < -2.0 {
        "Degrading (significant)"
    } else if percent < -0.5 {
        "Degrading (normal wear)"
    } else {
        "Stable"
    };
    println!("Battery Health Analysis ({} days)", args.days);
    println!("First reading       {}", first.date);
    println!("First avg capacity  {:.0} mAh", first.average_max_capacity);
    println!("Last reading        {}", last.date);
    println!("Last avg capacity   {:.0} mAh", last.average_max_capacity);
    println!("Change              {change:+.0} mAh ({percent:+.2}%)");
    println!("Status              {status}");
    println!("Days analyzed       {}", results.len());
    if results.len() > 3 {
        println!("\nDaily Trend (Last {} days)", results.len().min(7));
        println!("Date        Avg Capacity  Readings");
        for point in results.iter().rev().take(7).rev() {
            println!(
                "{}  {:>8.0} mAh  {:>8}",
                point.date, point.average_max_capacity, point.reading_count
            );
        }
    }
    Ok(())
}

fn config_command(command: ConfigCommand) -> Result<()> {
    let path = get_config_path();
    match command {
        ConfigCommand::Show => {
            let config = load_config();
            println!("Config file: {}", path.display());
            println!(
                "Config file exists: {}",
                if path.exists() { "yes" } else { "no" }
            );
            println!("Database path: {}", config.database_path.display());
            println!("Effective Configuration");
            println!(
                "Collection interval  {} seconds",
                config.collection_interval
            );
            println!("Stats history limit  {}", config.stats_history_limit);
            println!("Chart history limit  {}", config.chart_history_limit);
            println!("Default history limit  {}", config.default_history_limit);
            println!("Default export limit  {}", config.default_export_limit);
            println!("Log level  {}", config.log_level);
        }
        ConfigCommand::Init => {
            init_config(&path)?;
            println!("Created config file at {}", path.display());
        }
        ConfigCommand::Validate => {
            let result = validate_config_file(&path);
            if result.is_valid() {
                println!("Config file is valid: {}", result.path.display());
            } else {
                eprintln!("Config file is invalid: {}", result.path.display());
                for error in result.errors {
                    eprintln!("- {error}");
                }
                anyhow::bail!("configuration validation failed");
            }
        }
    }
    Ok(())
}

fn format_date(value: Option<chrono::DateTime<chrono::Utc>>) -> String {
    value
        .map(|date| date.to_rfc3339())
        .unwrap_or_else(|| "N/A".into())
}
