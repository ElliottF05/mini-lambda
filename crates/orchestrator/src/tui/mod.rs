// NOTE: this tui crate was my first true time using Claude Code for a fully self-contained feature.
// The rest of the code was almost all written by me, with AI assistance primarily being used to 
// bounce ideas off of and help with some debugging.

mod render;
pub mod state;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tui_logger::TuiWidgetState;

use crate::diagnostics::DiagnosticsStore;
use state::{CLIENT_COLS, JOB_COLS, Tab, TuiState, WORKER_COLS};

#[derive(PartialEq)]
enum Action {
    Continue,
    Quit,
}

/// Runs the TUI event loop. Returns when the user quits.
/// The gRPC server must already be running (spawned in main) before calling this.
pub async fn run(diagnostics: Arc<DiagnosticsStore>) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = TuiState::default();
    let log_state = TuiWidgetState::new();
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(250));

    loop {
        terminal.draw(|f| render::draw(f, &mut state, &diagnostics, &log_state))?;

        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key)))
                        if key.kind == crossterm::event::KeyEventKind::Press =>
                    {
                        if handle_key(key, &mut state) == Action::Quit {
                            break;
                        }
                    }
                    Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            _ = tick.tick() => {}
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn handle_key(key: crossterm::event::KeyEvent, state: &mut TuiState) -> Action {
    // Global quit
    if key.code == KeyCode::Char('q')
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
    {
        return Action::Quit;
    }

    // Tab switching via number keys
    match key.code {
        KeyCode::Char('1') => { state.tab = Tab::Dashboard; return Action::Continue; }
        KeyCode::Char('2') => { state.tab = Tab::Jobs;      return Action::Continue; }
        KeyCode::Char('3') => { state.tab = Tab::Workers;   return Action::Continue; }
        KeyCode::Char('4') => { state.tab = Tab::Clients;   return Action::Continue; }
        KeyCode::Char('5') => { state.tab = Tab::Logs;      return Action::Continue; }
        _ => {}
    }

    // Tab cycling
    match key.code {
        KeyCode::Tab => {
            state.tab = state.tab.next();
            return Action::Continue;
        }
        KeyCode::BackTab => {
            state.tab = state.tab.prev();
            return Action::Continue;
        }
        _ => {}
    }

    // Per-tab keys
    match state.tab {
        Tab::Jobs => handle_table_key(
            key,
            &mut state.jobs_selected,
            &mut state.jobs_sort_col,
            &mut state.jobs_sort_dir,
            JOB_COLS,
        ),
        Tab::Workers => handle_table_key(
            key,
            &mut state.workers_selected,
            &mut state.workers_sort_col,
            &mut state.workers_sort_dir,
            WORKER_COLS,
        ),
        Tab::Clients => handle_table_key(
            key,
            &mut state.clients_selected,
            &mut state.clients_sort_col,
            &mut state.clients_sort_dir,
            CLIENT_COLS,
        ),
        _ => {}
    }

    Action::Continue
}

fn handle_table_key(
    key: crossterm::event::KeyEvent,
    selected: &mut usize,
    sort_col: &mut usize,
    sort_dir: &mut state::SortDir,
    num_cols: usize,
) {
    match key.code {
        // Row navigation
        KeyCode::Down | KeyCode::Char('j') => *selected = selected.saturating_add(1),
        KeyCode::Up   | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
        // Sort column: left/right arrows or s (right alias)
        KeyCode::Right | KeyCode::Char('s') => *sort_col = (*sort_col + 1) % num_cols,
        KeyCode::Left                        => *sort_col = sort_col.checked_sub(1).unwrap_or(num_cols - 1),
        // Toggle sort direction
        KeyCode::Char('r') => *sort_dir = sort_dir.toggle(),
        _ => {}
    }
}
