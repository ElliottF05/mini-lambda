use std::time::{Duration, SystemTime};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Tabs};
use tui_logger::{TuiLoggerWidget, TuiWidgetState};
use uuid::Uuid;

use crate::diagnostics::{ClientInfo, DiagnosticsStore, JobInfo, JobState, WorkerInfo};
use crate::tui::state::{SortDir, Tab, TuiState};

// ── Colour palette ────────────────────────────────────────────────────────────

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const SUCCESS: Color = Color::Green;
const WARN: Color = Color::Yellow;
const ERR: Color = Color::Red;
const CANCEL: Color = Color::Magenta;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, state: &mut TuiState, diagnostics: &DiagnosticsStore, log_state: &TuiWidgetState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    draw_tabs(frame, chunks[0], state.tab);

    match state.tab {
        Tab::Dashboard => draw_dashboard(frame, chunks[1], diagnostics, log_state),
        Tab::Jobs      => draw_jobs(frame, chunks[1], state, diagnostics),
        Tab::Workers   => draw_workers(frame, chunks[1], state, diagnostics),
        Tab::Clients   => draw_clients(frame, chunks[1], state, diagnostics),
        Tab::Logs      => draw_logs(frame, chunks[1], log_state),
    }
}

// ── Tab bar ───────────────────────────────────────────────────────────────────

fn draw_tabs(frame: &mut Frame, area: Rect, active: Tab) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| Line::from(format!(" [{}] {} ", i + 1, t.title())))
        .collect();

    let tabs = Tabs::new(titles)
        .select(active.index())
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(DIM)))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider(Span::styled("│", Style::default().fg(DIM)));

    frame.render_widget(tabs, area);
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

fn draw_dashboard(frame: &mut Frame, area: Rect, diagnostics: &DiagnosticsStore, log_state: &TuiWidgetState) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(10)])
        .split(area);

    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(0)])
        .split(vsplit[0]);

    draw_orchestrator_stats(frame, hsplit[0], diagnostics);
    draw_jobs_panel(frame, hsplit[1], diagnostics);
    draw_mini_logs(frame, vsplit[1], log_state);
}

fn draw_orchestrator_stats(frame: &mut Frame, area: Rect, diagnostics: &DiagnosticsStore) {
    let now = SystemTime::now();
    let uptime = now.duration_since(diagnostics.started_at).unwrap_or_default();

    let mut queued = 0u32;
    let mut active = 0u32;
    let mut completed = 0u32;
    let mut failed = 0u32;
    let mut cancelled = 0u32;
    let total = diagnostics.jobs.len() as u32;

    for entry in diagnostics.jobs.iter() {
        match entry.state {
            JobState::Queued => queued += 1,
            JobState::Dispatched | JobState::Compiling | JobState::Executing => active += 1,
            JobState::Completed => completed += 1,
            JobState::Failed => failed += 1,
            JobState::Cancelled => cancelled += 1,
        }
    }

    let connected_workers = diagnostics.workers.iter()
        .filter(|w| w.disconnected_at.is_none())
        .count();

    let stat = |label: &'static str, value: String, style: Style| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("{:<11}", label), Style::default().fg(DIM)),
            Span::styled(value, style),
        ])
    };

    let lines = vec![
        stat("Uptime",    fmt_duration(uptime),           Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        stat("Workers",   connected_workers.to_string(),  Style::default().add_modifier(Modifier::BOLD)),
        stat("Total jobs", total.to_string(),             Style::default().add_modifier(Modifier::BOLD)),
        Line::from(Span::styled("─".repeat(20), Style::default().fg(DIM))),
        stat("Queued",    queued.to_string(),             Style::default().fg(WARN)),
        stat("Active",    active.to_string(),             Style::default().fg(ACCENT)),
        stat("Done",      completed.to_string(),          Style::default().fg(SUCCESS)),
        stat("Failed",    failed.to_string(),             Style::default().fg(ERR)),
        stat("Cancelled", cancelled.to_string(),          Style::default().fg(CANCEL)),
    ];

    let para = Paragraph::new(Text::from(lines))
        .block(styled_block("Orchestrator"));
    frame.render_widget(para, area);
}

fn draw_jobs_panel(frame: &mut Frame, area: Rect, diagnostics: &DiagnosticsStore) {
    let now = SystemTime::now();

    // All jobs, newest first
    let mut jobs: Vec<_> = diagnostics.jobs.iter().map(|j| j.clone()).collect();
    jobs.sort_by(|a, b| b.queued_at.cmp(&a.queued_at));
    jobs.truncate(50);

    if jobs.is_empty() {
        let para = Paragraph::new(Span::styled("no jobs yet", Style::default().fg(DIM)))
            .block(styled_block("Jobs"));
        frame.render_widget(para, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("ID").style(header_style()),
        Cell::from("Client").style(header_style()),
        Cell::from("State").style(header_style()),
        Cell::from("Age").style(header_style()),
    ]);

    let rows: Vec<Row> = jobs.iter().map(|j| {
        let age = now.duration_since(j.queued_at).unwrap_or_default();
        Row::new(vec![
            Cell::from(short_id(j.job_id)),
            Cell::from(short_addr(&j.client_address)),
            Cell::from(state_str(&j.state)).style(state_style(&j.state)),
            Cell::from(fmt_duration_short(age)),
        ])
    }).collect();

    let table = Table::new(
        rows,
        [Constraint::Length(10), Constraint::Min(14), Constraint::Length(11), Constraint::Length(8)],
    )
    .header(header)
    .block(styled_block("Jobs"));

    frame.render_widget(table, area);
}

fn draw_mini_logs(frame: &mut Frame, area: Rect, log_state: &TuiWidgetState) {
    let widget = TuiLoggerWidget::default()
        .block(styled_block("Logs"))
        .style_error(Style::default().fg(ERR).add_modifier(Modifier::BOLD))
        .style_warn(Style::default().fg(WARN).add_modifier(Modifier::BOLD))
        .style_info(Style::default().fg(SUCCESS))
        .style_debug(Style::default().fg(Color::Blue))
        .style_trace(Style::default().fg(DIM))
        .state(log_state);
    frame.render_widget(widget, area);
}

// ── Jobs tab ──────────────────────────────────────────────────────────────────

fn draw_jobs(frame: &mut Frame, area: Rect, state: &mut TuiState, diagnostics: &DiagnosticsStore) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(9)])
        .split(area);

    let now = SystemTime::now();
    let mut jobs: Vec<_> = diagnostics.jobs.iter().map(|j| j.clone()).collect();
    sort_jobs(&mut jobs, state.jobs_sort_col, state.jobs_sort_dir, now);

    // Clamp and write back so the key handler can't drift past the end
    let max = jobs.len().saturating_sub(1);
    state.jobs_selected = state.jobs_selected.min(max);
    let selected = state.jobs_selected;
    let detail = jobs.get(selected).cloned();

    let col_headers = ["Age", "Client", "Worker", "State", "Queue time", "Wkr time", "ID"];
    let sort_indicator = sort_indicator(state.jobs_sort_dir);

    let header = Row::new(col_headers.iter().enumerate().map(|(i, name)| {
        let label = if i == state.jobs_sort_col {
            format!("{name}{sort_indicator}")
        } else {
            name.to_string()
        };
        Cell::from(label).style(header_style())
    }));

    let rows: Vec<Row> = jobs.iter().map(|j| {
        let age = now.duration_since(j.queued_at).unwrap_or_default();
        let queue_time = queue_time(j, now);
        let worker_time = worker_time(j, now);
        Row::new(vec![
            Cell::from(fmt_duration_short(age)),
            Cell::from(short_addr(&j.client_address)),
            Cell::from(j.worker_address.as_deref().map(short_addr).unwrap_or_else(|| "—".into())),
            Cell::from(state_str(&j.state)).style(state_style(&j.state)),
            Cell::from(queue_time.map(fmt_duration_short).unwrap_or_else(|| "—".into())),
            Cell::from(worker_time.map(fmt_duration_short).unwrap_or_else(|| "—".into())),
            Cell::from(short_id(j.job_id)),
        ])
    }).collect();

    let mut table_state = TableState::default();
    if !jobs.is_empty() {
        table_state.select(Some(selected));
    }

    let title = format!("Jobs  ←/→ sort col  [r] reverse  ({} total)", jobs.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Min(14),
            Constraint::Min(14),
            Constraint::Length(11),
            Constraint::Length(11),
            Constraint::Length(9),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(styled_block(&title))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, vsplit[0], &mut table_state);
    draw_job_detail(frame, vsplit[1], detail.as_ref());
}

fn draw_job_detail(frame: &mut Frame, area: Rect, job: Option<&JobInfo>) {
    let content = match job {
        None => Text::from(Line::from(Span::styled("no selection", Style::default().fg(DIM)))),
        Some(j) => {
            let mut lines = vec![
                detail_line("ID",      j.job_id.to_string()),
                detail_line("State",   state_str(&j.state)),
                detail_line("Client",  j.client_address.clone()),
                detail_line("Worker",  j.worker_address.clone().unwrap_or_else(|| "—".into())),
                detail_line("Queued",  fmt_system_time(j.queued_at)),
            ];
            if let Some(t) = j.compiling_at  { lines.push(detail_line("Compiling", fmt_system_time(t))); }
            if let Some(t) = j.executing_at  { lines.push(detail_line("Executing", fmt_system_time(t))); }
            if let Some(t) = j.completed_at  { lines.push(detail_line("Completed", fmt_system_time(t))); }
            Text::from(lines)
        }
    };
    frame.render_widget(Paragraph::new(content).block(styled_block("Detail")), area);
}

// ── Workers tab ───────────────────────────────────────────────────────────────

fn draw_workers(frame: &mut Frame, area: Rect, state: &mut TuiState, diagnostics: &DiagnosticsStore) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(7)])
        .split(area);

    let mut workers: Vec<_> = diagnostics.workers.iter().map(|w| w.clone()).collect();
    sort_workers(&mut workers, state.workers_sort_col, state.workers_sort_dir);

    let max = workers.len().saturating_sub(1);
    state.workers_selected = state.workers_selected.min(max);
    let selected = state.workers_selected;
    let detail = workers.get(selected).cloned();

    let col_headers = ["Address", "Status", "Jobs rcvd", "Avg job", "Total time", "Connected"];
    let sort_indicator = sort_indicator(state.workers_sort_dir);

    let header = Row::new(col_headers.iter().enumerate().map(|(i, name)| {
        let label = if i == state.workers_sort_col {
            format!("{name}{sort_indicator}")
        } else {
            name.to_string()
        };
        Cell::from(label).style(header_style())
    }));

    let rows: Vec<Row> = workers.iter().map(|w| {
        let status = if w.disconnected_at.is_some() { "offline" } else { "online" };
        let status_style = if w.disconnected_at.is_some() {
            Style::default().fg(DIM)
        } else {
            Style::default().fg(SUCCESS)
        };
        let avg = if w.jobs_received > 0 {
            fmt_duration_short(w.total_job_time / w.jobs_received)
        } else {
            "—".into()
        };
        let connected_ago = SystemTime::now().duration_since(w.connected_at).unwrap_or_default();
        Row::new(vec![
            Cell::from(w.address.as_str()),
            Cell::from(status).style(status_style),
            Cell::from(w.jobs_received.to_string()),
            Cell::from(avg),
            Cell::from(fmt_duration_short(w.total_job_time)),
            Cell::from(fmt_duration_short(connected_ago)),
        ])
    }).collect();

    let mut table_state = TableState::default();
    if !workers.is_empty() {
        table_state.select(Some(selected));
    }

    let title = format!("Workers  ←/→ sort col  [r] reverse  ({} total)", workers.len());
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(11),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(styled_block(&title))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, vsplit[0], &mut table_state);
    draw_worker_detail(frame, vsplit[1], detail.as_ref());
}

fn draw_worker_detail(frame: &mut Frame, area: Rect, worker: Option<&WorkerInfo>) {
    let content = match worker {
        None => Text::from(Line::from(Span::styled("no selection", Style::default().fg(DIM)))),
        Some(w) => {
            let mut lines = vec![
                detail_line("Address",    w.address.clone()),
                detail_line("Jobs rcvd",  w.jobs_received.to_string()),
                detail_line("Total time", fmt_duration_short(w.total_job_time)),
                detail_line("Connected",  fmt_system_time(w.connected_at)),
            ];
            if let Some(t) = w.disconnected_at {
                lines.push(detail_line("Disconnected", fmt_system_time(t)));
            }
            Text::from(lines)
        }
    };
    frame.render_widget(Paragraph::new(content).block(styled_block("Detail")), area);
}

// ── Clients tab ───────────────────────────────────────────────────────────────

fn draw_clients(frame: &mut Frame, area: Rect, state: &mut TuiState, diagnostics: &DiagnosticsStore) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(7)])
        .split(area);

    let mut clients: Vec<_> = diagnostics.clients.iter().map(|c| c.clone()).collect();
    sort_clients(&mut clients, state.clients_sort_col, state.clients_sort_dir);

    let max = clients.len().saturating_sub(1);
    state.clients_selected = state.clients_selected.min(max);
    let selected = state.clients_selected;
    let detail = clients.get(selected).cloned();

    let col_headers = ["Address", "Jobs", "Avg queue", "Tot queue", "Avg worker", "Tot worker", "Connected"];
    let sort_indicator = sort_indicator(state.clients_sort_dir);

    let header = Row::new(col_headers.iter().enumerate().map(|(i, name)| {
        let label = if i == state.clients_sort_col {
            format!("{name}{sort_indicator}")
        } else {
            name.to_string()
        };
        Cell::from(label).style(header_style())
    }));

    let rows: Vec<Row> = clients.iter().map(|c| {
        let avg_queue = if c.jobs_submitted > 0 {
            fmt_duration_short(c.total_queue_time / c.jobs_submitted)
        } else {
            "—".into()
        };
        let avg_worker = if c.jobs_submitted > 0 {
            fmt_duration_short(c.total_worker_time / c.jobs_submitted)
        } else {
            "—".into()
        };
        let connected_ago = SystemTime::now().duration_since(c.connected_at).unwrap_or_default();
        Row::new(vec![
            Cell::from(c.address.as_str()),
            Cell::from(c.jobs_submitted.to_string()),
            Cell::from(avg_queue),
            Cell::from(fmt_duration_short(c.total_queue_time)),
            Cell::from(avg_worker),
            Cell::from(fmt_duration_short(c.total_worker_time)),
            Cell::from(fmt_duration_short(connected_ago)),
        ])
    }).collect();

    let mut table_state = TableState::default();
    if !clients.is_empty() {
        table_state.select(Some(selected));
    }

    let title = format!("Clients  ←/→ sort col  [r] reverse  ({} total)", clients.len());
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(6),
            Constraint::Length(11),
            Constraint::Length(11),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(11),
        ],
    )
    .header(header)
    .block(styled_block(&title))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, vsplit[0], &mut table_state);
    draw_client_detail(frame, vsplit[1], detail.as_ref());
}

fn draw_client_detail(frame: &mut Frame, area: Rect, client: Option<&ClientInfo>) {
    let content = match client {
        None => Text::from(Line::from(Span::styled("no selection", Style::default().fg(DIM)))),
        Some(c) => Text::from(vec![
            detail_line("Address",        c.address.clone()),
            detail_line("Jobs submitted", c.jobs_submitted.to_string()),
            detail_line("Total queue",    fmt_duration_short(c.total_queue_time)),
            detail_line("Total worker",   fmt_duration_short(c.total_worker_time)),
            detail_line("Connected",      fmt_system_time(c.connected_at)),
            detail_line("Last seen",      fmt_system_time(c.last_seen_at)),
        ]),
    };
    frame.render_widget(Paragraph::new(content).block(styled_block("Detail")), area);
}

// ── Logs tab ──────────────────────────────────────────────────────────────────

fn draw_logs(frame: &mut Frame, area: Rect, log_state: &TuiWidgetState) {
    let widget = TuiLoggerWidget::default()
        .block(styled_block("Logs"))
        .style_error(Style::default().fg(ERR).add_modifier(Modifier::BOLD))
        .style_warn(Style::default().fg(WARN).add_modifier(Modifier::BOLD))
        .style_info(Style::default().fg(SUCCESS))
        .style_debug(Style::default().fg(Color::Blue))
        .style_trace(Style::default().fg(DIM))
        .state(log_state);
    frame.render_widget(widget, area);
}

// ── Sorting ───────────────────────────────────────────────────────────────────

fn sort_jobs(jobs: &mut Vec<JobInfo>, col: usize, dir: SortDir, now: SystemTime) {
    jobs.sort_by(|a, b| {
        let ord = match col {
            0 => a.queued_at.cmp(&b.queued_at),                              // Age
            1 => a.client_address.cmp(&b.client_address),                    // Client
            2 => a.worker_address.cmp(&b.worker_address),                    // Worker
            3 => a.state.cmp(&b.state),                                      // State
            4 => queue_time(a, now).cmp(&queue_time(b, now)),                // Queue time
            5 => worker_time(a, now).cmp(&worker_time(b, now)),              // Wkr time
            _ => a.job_id.cmp(&b.job_id),                                    // ID (col 6)
        };
        if dir == SortDir::Desc { ord.reverse() } else { ord }
    });
}

fn sort_workers(workers: &mut Vec<WorkerInfo>, col: usize, dir: SortDir) {
    workers.sort_by(|a, b| {
        let ord = match col {
            0 => a.address.cmp(&b.address),
            1 => a.disconnected_at.is_none().cmp(&b.disconnected_at.is_none()),
            2 => a.jobs_received.cmp(&b.jobs_received),
            3 => {
                let avg_a = if a.jobs_received > 0 { a.total_job_time / a.jobs_received } else { Duration::ZERO };
                let avg_b = if b.jobs_received > 0 { b.total_job_time / b.jobs_received } else { Duration::ZERO };
                avg_a.cmp(&avg_b)
            }
            4 => a.total_job_time.cmp(&b.total_job_time),
            _ => a.connected_at.cmp(&b.connected_at),                        // Connected (col 5)
        };
        if dir == SortDir::Desc { ord.reverse() } else { ord }
    });
}

fn sort_clients(clients: &mut Vec<ClientInfo>, col: usize, dir: SortDir) {
    clients.sort_by(|a, b| {
        let ord = match col {
            0 => a.address.cmp(&b.address),
            1 => a.jobs_submitted.cmp(&b.jobs_submitted),
            2 => {
                let avg_a = if a.jobs_submitted > 0 { a.total_queue_time / a.jobs_submitted } else { Duration::ZERO };
                let avg_b = if b.jobs_submitted > 0 { b.total_queue_time / b.jobs_submitted } else { Duration::ZERO };
                avg_a.cmp(&avg_b)
            }
            3 => a.total_queue_time.cmp(&b.total_queue_time),
            4 => {
                let avg_a = if a.jobs_submitted > 0 { a.total_worker_time / a.jobs_submitted } else { Duration::ZERO };
                let avg_b = if b.jobs_submitted > 0 { b.total_worker_time / b.jobs_submitted } else { Duration::ZERO };
                avg_a.cmp(&avg_b)
            }
            5 => a.total_worker_time.cmp(&b.total_worker_time),
            _ => a.connected_at.cmp(&b.connected_at),                        // Connected (col 6)
        };
        if dir == SortDir::Desc { ord.reverse() } else { ord }
    });
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn queue_time(job: &JobInfo, now: SystemTime) -> Option<Duration> {
    match job.state {
        JobState::Queued | JobState::Dispatched => {
            Some(now.duration_since(job.queued_at).unwrap_or_default())
        }
        _ => {
            let end = job.compiling_at.or(job.executing_at).or(job.completed_at);
            end.map(|e| e.duration_since(job.queued_at).unwrap_or_default())
        }
    }
}

fn worker_time(job: &JobInfo, now: SystemTime) -> Option<Duration> {
    let start = job.compiling_at.or(job.executing_at)?;
    let end = job.completed_at.unwrap_or(now);
    Some(end.duration_since(start).unwrap_or_default())
}

fn fmt_duration(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

fn fmt_duration_short(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        fmt_duration(d)
    }
}

fn fmt_system_time(t: SystemTime) -> String {
    let elapsed = SystemTime::now().duration_since(t).unwrap_or_default();
    format!("{} ago", fmt_duration_short(elapsed))
}

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

fn short_addr(addr: &str) -> String {
    if addr.len() > 18 {
        format!("…{}", &addr[addr.len() - 17..])
    } else {
        addr.to_string()
    }
}

fn state_str(state: &JobState) -> &'static str {
    match state {
        JobState::Queued     => "Queued",
        JobState::Dispatched => "Dispatched",
        JobState::Compiling  => "Compiling",
        JobState::Executing  => "Executing",
        JobState::Completed  => "Completed",
        JobState::Failed     => "Failed",
        JobState::Cancelled  => "Cancelled",
    }
}

fn state_style(state: &JobState) -> Style {
    match state {
        JobState::Queued | JobState::Dispatched => Style::default().fg(WARN),
        JobState::Compiling | JobState::Executing => Style::default().fg(ACCENT),
        JobState::Completed => Style::default().fg(SUCCESS),
        JobState::Failed => Style::default().fg(ERR),
        JobState::Cancelled => Style::default().fg(CANCEL),
    }
}

fn sort_indicator(dir: SortDir) -> &'static str {
    match dir {
        SortDir::Asc  => " ▲",
        SortDir::Desc => " ▼",
    }
}

fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .title(Span::styled(format!(" {title} "), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM))
}

fn header_style() -> Style {
    Style::default().fg(Color::White).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

fn detail_line(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<14} "), Style::default().fg(DIM)),
        Span::raw(value.into()),
    ])
}
