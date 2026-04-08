use std::time::{Duration, SystemTime};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Tabs};
use tui_logger::{TuiLoggerSmartWidget, TuiLoggerWidget, TuiWidgetState};
use uuid::Uuid;

use crate::diagnostics::{ClientInfo, DiagnosticsStore, JobInfo, JobState, WorkerInfo};
use crate::tui::state::{ClientSortCol, JobSortCol, SortDir, Tab, TuiState, WorkerSortCol};

// ── Colour palette ────────────────────────────────────────────────────────────

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const SUCCESS: Color = Color::Green;
const WARN: Color = Color::Yellow;
const ERR: Color = Color::Red;
const CANCEL: Color = Color::Magenta;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, state: &TuiState, diagnostics: &DiagnosticsStore, log_state: &TuiWidgetState) {
    let area = frame.area();

    // Split: tab bar (3 rows) + content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    draw_tabs(frame, chunks[0], state.tab);

    match state.tab {
        Tab::Dashboard => draw_dashboard(frame, chunks[1], state, diagnostics, log_state),
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
        .map(|t| Line::from(format!(" {} ", t.title())))
        .collect();

    let tabs = Tabs::new(titles)
        .select(active.index())
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(DIM)))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider(Span::styled("│", Style::default().fg(DIM)));

    frame.render_widget(tabs, area);
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

fn draw_dashboard(frame: &mut Frame, area: Rect, _state: &TuiState, diagnostics: &DiagnosticsStore, log_state: &TuiWidgetState) {
    // Vertical: top section (stats + active jobs) / bottom (mini log)
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(10)])
        .split(area);

    // Top: stats panel (left) + active jobs (right)
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(0)])
        .split(vsplit[0]);

    draw_orchestrator_stats(frame, hsplit[0], diagnostics);
    draw_active_jobs(frame, hsplit[1], diagnostics);
    draw_mini_logs(frame, vsplit[1], log_state);
}

fn draw_orchestrator_stats(frame: &mut Frame, area: Rect, diagnostics: &DiagnosticsStore) {
    let now = SystemTime::now();
    let uptime = now.duration_since(diagnostics.started_at).unwrap_or_default();

    // Count jobs per state
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

    let lines = vec![
        Line::from(vec![
            Span::styled("Uptime   ", Style::default().fg(DIM)),
            Span::styled(fmt_duration(uptime), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Workers  ", Style::default().fg(DIM)),
            Span::styled(connected_workers.to_string(), Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Total jobs", Style::default().fg(DIM)),
            Span::raw(" "),
            Span::styled(total.to_string(), Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Queued   ", Style::default().fg(DIM)),
            Span::styled(queued.to_string(), Style::default().fg(WARN)),
        ]),
        Line::from(vec![
            Span::styled("  Active   ", Style::default().fg(DIM)),
            Span::styled(active.to_string(), Style::default().fg(ACCENT)),
        ]),
        Line::from(vec![
            Span::styled("  Done     ", Style::default().fg(DIM)),
            Span::styled(completed.to_string(), Style::default().fg(SUCCESS)),
        ]),
        Line::from(vec![
            Span::styled("  Failed   ", Style::default().fg(DIM)),
            Span::styled(failed.to_string(), Style::default().fg(ERR)),
        ]),
        Line::from(vec![
            Span::styled("  Cancelled", Style::default().fg(DIM)),
            Span::styled(cancelled.to_string(), Style::default().fg(CANCEL)),
        ]),
    ];

    let block = styled_block("Orchestrator");
    let para = Paragraph::new(Text::from(lines))
        .block(block)
        .style(Style::default());
    frame.render_widget(para, area);
}

fn draw_active_jobs(frame: &mut Frame, area: Rect, diagnostics: &DiagnosticsStore) {
    let now = SystemTime::now();

    // Collect non-terminal jobs sorted by queued_at
    let mut jobs: Vec<_> = diagnostics.jobs.iter()
        .filter(|j| !matches!(j.state, JobState::Completed | JobState::Failed | JobState::Cancelled))
        .map(|j| j.clone())
        .collect();
    jobs.sort_by_key(|j| j.queued_at);

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

    let empty_hint = if rows.is_empty() {
        vec![Row::new(vec![Cell::from("no active jobs").style(Style::default().fg(DIM))])]
    } else {
        vec![]
    };

    let table = Table::new(
        if rows.is_empty() { empty_hint } else { rows },
        [Constraint::Length(10), Constraint::Min(16), Constraint::Length(12), Constraint::Length(8)],
    )
    .header(header)
    .block(styled_block("Active Jobs"))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(table, area);
}

fn draw_mini_logs(frame: &mut Frame, area: Rect, log_state: &TuiWidgetState) {
    let widget = TuiLoggerWidget::default()
        .block(styled_block("Logs"))
        .style_error(Style::default().fg(ERR))
        .style_warn(Style::default().fg(WARN))
        .style_info(Style::default().fg(Color::White))
        .style_debug(Style::default().fg(DIM))
        .style_trace(Style::default().fg(DIM))
        .state(log_state);
    frame.render_widget(widget, area);
}

// ── Jobs tab ──────────────────────────────────────────────────────────────────

fn draw_jobs(frame: &mut Frame, area: Rect, state: &TuiState, diagnostics: &DiagnosticsStore) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(9)])
        .split(area);

    let now = SystemTime::now();
    let mut jobs: Vec<_> = diagnostics.jobs.iter().map(|j| j.clone()).collect();
    sort_jobs(&mut jobs, state.jobs_sort_col, state.jobs_sort_dir, now);

    let selected = state.jobs_selected.min(jobs.len().saturating_sub(1));
    let detail = jobs.get(selected).cloned();

    let sort_indicator = sort_indicator(state.jobs_sort_dir);
    let col_headers = [
        ("ID",         JobSortCol::QueuedAt),
        ("Client",     JobSortCol::Client),
        ("Worker",     JobSortCol::Worker),
        ("State",      JobSortCol::State),
        ("Age",        JobSortCol::QueuedAt),
        ("Queue time", JobSortCol::QueueTime),
        ("Wkr time",   JobSortCol::WorkerTime),
    ];

    let header = Row::new(col_headers.iter().map(|(name, col)| {
        let label = if *col == state.jobs_sort_col {
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
            Cell::from(short_id(j.job_id)),
            Cell::from(short_addr(&j.client_address)),
            Cell::from(j.worker_address.as_deref().map(short_addr).unwrap_or_else(|| "—".into())),
            Cell::from(state_str(&j.state)).style(state_style(&j.state)),
            Cell::from(fmt_duration_short(age)),
            Cell::from(queue_time.map(fmt_duration_short).unwrap_or_else(|| "—".into())),
            Cell::from(worker_time.map(fmt_duration_short).unwrap_or_else(|| "—".into())),
        ])
    }).collect();

    let mut table_state = TableState::default();
    if !jobs.is_empty() {
        table_state.select(Some(selected));
    }

    let title = format!("Jobs  [s] cycle sort  [r] reverse  ({} total)", jobs.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(16),
            Constraint::Min(16),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(styled_block(&title))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, vsplit[0], &mut table_state);
    draw_job_detail(frame, vsplit[1], detail.as_ref(), now);
}

fn draw_job_detail(frame: &mut Frame, area: Rect, job: Option<&JobInfo>, _now: SystemTime) {
    let content = match job {
        None => Text::from(Line::from(Span::styled("no selection", Style::default().fg(DIM)))),
        Some(j) => {
            let mut lines = vec![
                detail_line("ID",       j.job_id.to_string()),
                detail_line("State",    state_str(&j.state)),
                detail_line("Client",   j.client_address.clone()),
                detail_line("Worker",   j.worker_address.clone().unwrap_or_else(|| "—".into())),
                detail_line("Queued",   fmt_system_time(j.queued_at)),
            ];
            if let Some(t) = j.compiling_at {
                lines.push(detail_line("Compiling", fmt_system_time(t)));
            }
            if let Some(t) = j.executing_at {
                lines.push(detail_line("Executing", fmt_system_time(t)));
            }
            if let Some(t) = j.completed_at {
                lines.push(detail_line("Completed", fmt_system_time(t)));
            }
            Text::from(lines)
        }
    };
    let para = Paragraph::new(content).block(styled_block("Detail"));
    frame.render_widget(para, area);
}

// ── Workers tab ───────────────────────────────────────────────────────────────

fn draw_workers(frame: &mut Frame, area: Rect, state: &TuiState, diagnostics: &DiagnosticsStore) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(7)])
        .split(area);

    let mut workers: Vec<_> = diagnostics.workers.iter().map(|w| w.clone()).collect();
    sort_workers(&mut workers, state.workers_sort_col, state.workers_sort_dir);

    let selected = state.workers_selected.min(workers.len().saturating_sub(1));
    let detail = workers.get(selected).cloned();

    let sort_indicator = sort_indicator(state.workers_sort_dir);
    let col_headers = [
        ("Address",    WorkerSortCol::ConnectedAt),
        ("Status",     WorkerSortCol::Status),
        ("Jobs rcvd",  WorkerSortCol::JobsReceived),
        ("Avg job",    WorkerSortCol::TotalJobTime),
        ("Total time", WorkerSortCol::TotalJobTime),
        ("Connected",  WorkerSortCol::ConnectedAt),
    ];

    let header = Row::new(col_headers.iter().map(|(name, col)| {
        let label = if *col == state.workers_sort_col {
            format!("{name}{sort_indicator}")
        } else {
            name.to_string()
        };
        Cell::from(label).style(header_style())
    }));

    let rows: Vec<Row> = workers.iter().map(|w| {
        let status = if w.disconnected_at.is_some() { "offline" } else { "online" };
        let status_style = if w.disconnected_at.is_some() { Style::default().fg(DIM) } else { Style::default().fg(SUCCESS) };
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

    let title = format!("Workers  [s] cycle sort  [r] reverse  ({} total)", workers.len());
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(10),
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
    let para = Paragraph::new(content).block(styled_block("Detail"));
    frame.render_widget(para, area);
}

// ── Clients tab ───────────────────────────────────────────────────────────────

fn draw_clients(frame: &mut Frame, area: Rect, state: &TuiState, diagnostics: &DiagnosticsStore) {
    let vsplit = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(7)])
        .split(area);

    let mut clients: Vec<_> = diagnostics.clients.iter().map(|c| c.clone()).collect();
    sort_clients(&mut clients, state.clients_sort_col, state.clients_sort_dir);

    let selected = state.clients_selected.min(clients.len().saturating_sub(1));
    let detail = clients.get(selected).cloned();

    let sort_indicator = sort_indicator(state.clients_sort_dir);
    let col_headers = [
        ("Address",       ClientSortCol::ConnectedAt),
        ("Jobs submitted",ClientSortCol::JobsSubmitted),
        ("Avg queue",     ClientSortCol::TotalQueueTime),
        ("Total queue",   ClientSortCol::TotalQueueTime),
        ("Avg worker",    ClientSortCol::TotalWorkerTime),
        ("Total worker",  ClientSortCol::TotalWorkerTime),
        ("Connected",     ClientSortCol::ConnectedAt),
    ];

    let header = Row::new(col_headers.iter().map(|(name, col)| {
        let label = if *col == state.clients_sort_col {
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

    let title = format!("Clients  [s] cycle sort  [r] reverse  ({} total)", clients.len());
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(13),
            Constraint::Length(12),
            Constraint::Length(13),
            Constraint::Length(12),
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
    let para = Paragraph::new(content).block(styled_block("Detail"));
    frame.render_widget(para, area);
}

// ── Logs tab ──────────────────────────────────────────────────────────────────

fn draw_logs(frame: &mut Frame, area: Rect, log_state: &TuiWidgetState) {
    let widget = TuiLoggerSmartWidget::default()
        .style_error(Style::default().fg(ERR))
        .style_warn(Style::default().fg(WARN))
        .style_info(Style::default().fg(Color::White))
        .style_debug(Style::default().fg(DIM))
        .style_trace(Style::default().fg(DIM))
        .state(log_state);
    frame.render_widget(widget, area);
}

// ── Sorting helpers ───────────────────────────────────────────────────────────

fn sort_jobs(jobs: &mut Vec<JobInfo>, col: JobSortCol, dir: SortDir, now: SystemTime) {
    jobs.sort_by(|a, b| {
        let ord = match col {
            JobSortCol::QueuedAt  => a.queued_at.cmp(&b.queued_at),
            JobSortCol::State     => a.state.cmp(&b.state),
            JobSortCol::Client    => a.client_address.cmp(&b.client_address),
            JobSortCol::Worker    => a.worker_address.cmp(&b.worker_address),
            JobSortCol::QueueTime => queue_time(a, now).cmp(&queue_time(b, now)),
            JobSortCol::WorkerTime => worker_time(a, now).cmp(&worker_time(b, now)),
        };
        if dir == SortDir::Desc { ord.reverse() } else { ord }
    });
}

fn sort_workers(workers: &mut Vec<WorkerInfo>, col: WorkerSortCol, dir: SortDir) {
    workers.sort_by(|a, b| {
        let ord = match col {
            WorkerSortCol::ConnectedAt  => a.connected_at.cmp(&b.connected_at),
            WorkerSortCol::JobsReceived => a.jobs_received.cmp(&b.jobs_received),
            WorkerSortCol::TotalJobTime => a.total_job_time.cmp(&b.total_job_time),
            WorkerSortCol::Status       => a.disconnected_at.is_none().cmp(&b.disconnected_at.is_none()),
        };
        if dir == SortDir::Desc { ord.reverse() } else { ord }
    });
}

fn sort_clients(clients: &mut Vec<ClientInfo>, col: ClientSortCol, dir: SortDir) {
    clients.sort_by(|a, b| {
        let ord = match col {
            ClientSortCol::ConnectedAt    => a.connected_at.cmp(&b.connected_at),
            ClientSortCol::JobsSubmitted  => a.jobs_submitted.cmp(&b.jobs_submitted),
            ClientSortCol::TotalQueueTime => a.total_queue_time.cmp(&b.total_queue_time),
            ClientSortCol::TotalWorkerTime => a.total_worker_time.cmp(&b.total_worker_time),
        };
        if dir == SortDir::Desc { ord.reverse() } else { ord }
    });
}

// ── Utility helpers ───────────────────────────────────────────────────────────

fn queue_time(job: &JobInfo, now: SystemTime) -> Option<Duration> {
    // Queue time: queued_at → dispatched (approximated by compiling_at or executing_at)
    // If still queued/dispatched, measure up to now
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
    // Format as elapsed since t
    let elapsed = SystemTime::now().duration_since(t).unwrap_or_default();
    format!("{} ago", fmt_duration_short(elapsed))
}

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

fn short_addr(addr: &str) -> String {
    // Trim to last 20 chars if long
    if addr.len() > 20 {
        format!("…{}", &addr[addr.len() - 19..])
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
