use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph, Wrap};

use crate::collector::PowerCollector;
use crate::config::PwerConfig;
use crate::database::{Database, Statistics};
use crate::models::PowerReading;

const STACKED_MIN_WIDTH: u16 = 60;
const STACKED_MIN_HEIGHT: u16 = 40;
const SIDE_BY_SIDE_MIN_WIDTH: u16 = 71;
const SIDE_BY_SIDE_MIN_HEIGHT: u16 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiLayoutMode {
    Stacked,
    SideBySide,
    CompactStacked,
}

pub fn select_tui_layout(width: u16, height: u16) -> TuiLayoutMode {
    if width >= STACKED_MIN_WIDTH && height >= STACKED_MIN_HEIGHT {
        TuiLayoutMode::Stacked
    } else if width >= SIDE_BY_SIDE_MIN_WIDTH && height >= SIDE_BY_SIDE_MIN_HEIGHT {
        TuiLayoutMode::SideBySide
    } else {
        TuiLayoutMode::CompactStacked
    }
}

pub fn run_tui(
    config: PwerConfig,
    collector: Box<dyn PowerCollector>,
    database: Database,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, config, collector, database);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: PwerConfig,
    collector: Box<dyn PowerCollector>,
    database: Database,
) -> Result<()> {
    let (reading_tx, reading_rx) = mpsc::channel();
    let (refresh_tx, refresh_rx) = mpsc::sync_channel(1);
    let running = Arc::new(AtomicBool::new(true));
    let worker_running = Arc::clone(&running);
    let interval = Duration::from_secs_f64(config.collection_interval);
    let worker = thread::spawn(move || {
        while worker_running.load(Ordering::Relaxed) {
            let reading = collector.collect();
            if reading_tx.send(reading).is_err() {
                break;
            }
            let _ = refresh_rx.recv_timeout(interval);
        }
    });

    let mut current = None;
    let mut status = String::from("Waiting for data...");
    loop {
        while let Ok(result) = reading_rx.try_recv() {
            match result {
                Ok(reading) => {
                    match database.insert_reading(&reading) {
                        Ok(_) => status = "Reading saved".into(),
                        Err(error) => status = format!("Warning: failed to save reading: {error}"),
                    }
                    current = Some(reading);
                }
                Err(error) => status = format!("Collection error: {error}"),
            }
        }
        let statistics = database.get_statistics(Some(config.stats_history_limit))?;
        let history = database.query_history(Some(config.chart_history_limit))?;
        terminal.draw(|frame| draw(frame, current.as_ref(), &statistics, &history, &status))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('r') => {
                    let _ = refresh_tx.try_send(());
                    status = "Refreshing...".into();
                }
                KeyCode::Char('c') => match database.clear_history() {
                    Ok(deleted) => {
                        current = None;
                        status = format!("Cleared {deleted} historical readings");
                    }
                    Err(error) => status = format!("Failed to clear history: {error}"),
                },
                _ => {}
            }
        }
    }
    running.store(false, Ordering::Relaxed);
    let _ = refresh_tx.try_send(());
    let _ = worker.join();
    Ok(())
}

fn draw(
    frame: &mut ratatui::Frame<'_>,
    current: Option<&PowerReading>,
    statistics: &Statistics,
    history: &[PowerReading],
    status: &str,
) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());
    frame.render_widget(
        Paragraph::new(" pwer - macOS Power Monitoring").style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        root[0],
    );
    frame.render_widget(
        Paragraph::new(format!(" Q Quit  R Refresh  C Clear History  | {status}"))
            .style(Style::default().fg(Color::DarkGray)),
        root[2],
    );

    match select_tui_layout(frame.area().width, frame.area().height) {
        TuiLayoutMode::SideBySide => {
            let rows =
                Layout::vertical([Constraint::Length(10), Constraint::Min(5)]).split(root[1]);
            let summary =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(rows[0]);
            render_live(frame, summary[0], current);
            render_stats(frame, summary[1], statistics);
            render_chart(frame, rows[1], history);
        }
        TuiLayoutMode::Stacked => {
            let rows = Layout::vertical([
                Constraint::Length(11),
                Constraint::Length(11),
                Constraint::Min(13),
            ])
            .split(root[1]);
            render_live(frame, rows[0], current);
            render_stats(frame, rows[1], statistics);
            render_chart(frame, rows[2], history);
        }
        TuiLayoutMode::CompactStacked => {
            let rows = Layout::vertical([
                Constraint::Length(7),
                Constraint::Length(7),
                Constraint::Min(3),
            ])
            .split(root[1]);
            render_live(frame, rows[0], current);
            render_stats(frame, rows[1], statistics);
            render_chart(frame, rows[2], history);
        }
    }
}

fn render_live(frame: &mut ratatui::Frame<'_>, area: Rect, reading: Option<&PowerReading>) {
    let block = Block::default()
        .title(" Live Data ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    let Some(reading) = reading else {
        frame.render_widget(Paragraph::new("Waiting for data...").block(block), area);
        return;
    };
    let (status, color) = if reading.is_charging {
        ("⚡ Charging", Color::Green)
    } else if reading.external_connected {
        ("🔌 On AC Power (Not Charging)", Color::Yellow)
    } else {
        ("🔋 On Battery", Color::Red)
    };
    let power = if reading.watts_negotiated > 0 {
        format!(
            "{:.1}W / {}W max",
            reading.watts_actual, reading.watts_negotiated
        )
    } else {
        format!("{:.1}W", reading.watts_actual)
    };
    let mut lines = vec![
        Line::from(Span::styled(
            status,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("Power: {power}")),
        Line::from(format!(
            "Battery: {}% ({} / {} mAh)",
            reading.battery_percent, reading.current_capacity, reading.max_capacity
        )),
        Line::from(format!(
            "Electrical: {:.2}V × {:+.2}A",
            reading.voltage, reading.amperage
        )),
    ];
    if let Some(charger) = &reading.charger_name {
        lines.push(Line::from(format!(
            "Charger: {}{}",
            charger,
            reading
                .charger_manufacturer
                .as_ref()
                .map(|m| format!(" ({m})"))
                .unwrap_or_default()
        )));
    }
    lines.push(Line::from(format!(
        "Time: {}",
        reading.timestamp.format("%Y-%m-%d %H:%M:%S")
    )));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn render_stats(frame: &mut ratatui::Frame<'_>, area: Rect, stats: &Statistics) {
    let block = Block::default()
        .title(" Statistics ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    if stats.count == 0 {
        frame.render_widget(
            Paragraph::new("No historical data available").block(block),
            area,
        );
        return;
    }
    let lines = vec![
        Line::from(format!("Last {} readings", stats.count)),
        Line::from(format!(
            "Latest: {}",
            stats
                .latest
                .map(|v| v.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "N/A".into())
        )),
        Line::from(format!(
            "Earliest: {}",
            stats
                .earliest
                .map(|v| v.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "N/A".into())
        )),
        Line::from(format!(
            "Avg / Min / Max: {:.1}W / {:.1}W / {:.1}W",
            stats.avg_watts, stats.min_watts, stats.max_watts
        )),
        Line::from(format!("Avg Battery: {:.1}%", stats.avg_battery)),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn render_chart(frame: &mut ratatui::Frame<'_>, area: Rect, history: &[PowerReading]) {
    let block = Block::default()
        .title(format!(" Power Over Time (Last {}) ", history.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    if history.is_empty() || area.height < 4 {
        frame.render_widget(Paragraph::new("No chart data available").block(block), area);
        return;
    }
    let points: Vec<(f64, f64)> = history
        .iter()
        .rev()
        .enumerate()
        .map(|(index, reading)| (index as f64, reading.watts_actual))
        .collect();
    let negotiated: Vec<(f64, f64)> = history
        .iter()
        .rev()
        .enumerate()
        .map(|(index, reading)| (index as f64, reading.watts_negotiated as f64))
        .collect();
    let min_y = points
        .iter()
        .chain(&negotiated)
        .map(|point| point.1)
        .fold(0.0, f64::min);
    let max_y = points
        .iter()
        .chain(&negotiated)
        .map(|point| point.1)
        .fold(1.0, f64::max);
    let datasets = vec![
        Dataset::default()
            .name("Power (W)")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Red))
            .data(&points),
        Dataset::default()
            .name("Max Power (W)")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Blue))
            .data(&negotiated),
    ];
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .title("Oldest → Newest")
                .bounds([0.0, points.len().saturating_sub(1).max(1) as f64]),
        )
        .y_axis(
            Axis::default()
                .title("W")
                .bounds([min_y.floor(), max_y.ceil().max(min_y + 1.0)]),
        );
    frame.render_widget(chart, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_layout_matches_source_thresholds() {
        assert_eq!(select_tui_layout(80, 40), TuiLayoutMode::Stacked);
        assert_eq!(select_tui_layout(60, 40), TuiLayoutMode::Stacked);
        assert_eq!(select_tui_layout(80, 24), TuiLayoutMode::SideBySide);
        assert_eq!(select_tui_layout(120, 24), TuiLayoutMode::SideBySide);
        assert_eq!(select_tui_layout(60, 24), TuiLayoutMode::CompactStacked);
    }
}
