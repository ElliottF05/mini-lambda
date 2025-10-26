use std::{io, time::{Duration, SystemTime}};

use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::{execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode}};
use mini_lambda_proto::MonitoringInfo;
use ratatui::{backend::CrosstermBackend, Terminal};
use reqwest::Client;
use tokio::time::{interval, Instant};
use crossterm::event;
use std::time::UNIX_EPOCH;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::app::App;

fn fmt_system_time(st: SystemTime) -> String {
    DateTime::<Utc>::from(st).format("%Y-%m-%d %H:%M:%S").to_string()
}

pub async fn run(orchestrator: String) -> Result<()> {
    let monitor_url = format!("{}/monitoring_info", orchestrator.trim_end_matches('/'));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
    let mut app = App::new();

    let mut ticker = interval(Duration::from_secs(1));
    let mut last_tick = Instant::now();

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match client.get(&monitor_url).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            match resp.json::<MonitoringInfo>().await {
                                Ok(m) => {
                                    app.last_error = None;
                                    // process snapshot so we detect removed pending jobs -> dispatched
                                    app.process_snapshot(m.workers, m.pending);
                                }
                                Err(e) => {
                                    app.last_error = Some(format!("invalid monitor JSON: {}", e));
                                }
                            }
                        } else {
                            let status = resp.status();
                            let body = resp.text().await.unwrap_or_default();
                            app.last_error = Some(format!("monitor returned {}: {}", status, body));
                        }
                    }
                    Err(e) => {
                        app.last_error = Some(format!("monitor request failed: {}", e));
                    }
                }
                last_tick = Instant::now();
            }
            // non-blocking poll for keyboard events
            evt = tokio::task::spawn_blocking(|| {
                if event::poll(std::time::Duration::from_millis(10)).unwrap_or(false) {
                    Some(event::read())
                } else {
                    None
                }
            }) => {
                if let Ok(Some(Ok(ev))) = evt {
                    if let event::Event::Key(key) = ev {
                        use crossterm::event::{KeyCode, KeyModifiers};
                        if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)) {
                            break;
                        }
                    }
                }
            }
        }

        terminal.draw(|f| {
            let area = f.area();

            // layout: header, jobs table (full width), workers table (full width)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(12),
                    Constraint::Length(7),
                ])
                .split(area);

            // header
            let header_text = Span::styled(
                format!("mini-lambda orchestrator monitor — {} workers — {} queued",
                    app.workers.len(), app.pending.len()),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            );
            let header = Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(header, chunks[0]);

            // Job table: pending (oldest first) on top, dispatched (most recent first) after
            let mut job_rows: Vec<Row> = Vec::new();

            // Pending: sort by submitted_at ascending (oldest first)
            let mut pending = app.pending.clone();
            pending.sort_by_key(|j| j.submitted_at.duration_since(UNIX_EPOCH).unwrap_or_default());
            for j in &pending {
                let id_short = j.job_id.to_string().split('-').next().unwrap_or("").to_string();
                let when = fmt_system_time(j.submitted_at);
                let status_cell = Cell::from("PENDING").style(Style::default().fg(Color::Yellow));
                job_rows.push(Row::new(vec![
                    Cell::from(id_short),
                    status_cell,
                    Cell::from(when),
                    Cell::from("-"), // worker
                ]));
            }

            // Dispatched: use dispatched_jobs history (most-recent first)
            for d in app.dispatched_jobs.iter().take(200) {
                job_rows.push(Row::new(vec![
                    Cell::from(d.job_id.to_string().split('-').next().unwrap_or("").to_string()),
                    Cell::from("DISPATCHED").style(Style::default().fg(Color::Green)),
                    Cell::from(d.when.format("%Y-%m-%d %H:%M:%S").to_string()),
                    Cell::from(d.worker.clone()),
                ]));
            }

            let job_table = Table::new(job_rows, &[Constraint::Length(12), Constraint::Length(12), Constraint::Length(20), Constraint::Percentage(100)])
                .header(Row::new(vec![Cell::from("Job ID"), Cell::from("Status"), Cell::from("When"), Cell::from("Worker")] ))
                .block(Block::default().borders(Borders::ALL).title("Jobs"));
            f.render_widget(job_table, chunks[1]);

            // Worker table below
            let mut rows = vec![];
            for w in &app.workers {
                let id_short = w.id.to_string().split('-').next().unwrap_or("").to_string();
                let last_seen = fmt_system_time(w.last_seen);
                rows.push(Row::new(vec![
                    Cell::from(w.endpoint.clone()),
                    Cell::from(id_short),
                    Cell::from(w.credits.to_string()),
                    Cell::from(last_seen),
                ]));
            }
            let worker_table = Table::new(rows, &[Constraint::Percentage(50), Constraint::Length(12), Constraint::Length(8), Constraint::Length(18)])
                .header(Row::new(vec![Cell::from("Endpoint"), Cell::from("ID"), Cell::from("Credits"), Cell::from("Last seen")]))
                .block(Block::default().borders(Borders::ALL).title("Workers"));
            f.render_widget(worker_table, chunks[2]);
        })?;
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
