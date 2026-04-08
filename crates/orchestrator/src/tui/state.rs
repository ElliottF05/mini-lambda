/// Number of sortable columns in each table — used by the key handler for wrapping.
pub const JOB_COLS: usize = 7;
pub const WORKER_COLS: usize = 6;
pub const CLIENT_COLS: usize = 7;

/// Which tab is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Dashboard,
    Jobs,
    Workers,
    Clients,
    Logs,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[
        Tab::Dashboard,
        Tab::Jobs,
        Tab::Workers,
        Tab::Clients,
        Tab::Logs,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Jobs      => "Jobs",
            Tab::Workers   => "Workers",
            Tab::Clients   => "Clients",
            Tab::Logs      => "Logs",
        }
    }

    pub fn index(self) -> usize {
        Tab::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    pub fn from_index(i: usize) -> Self {
        Tab::ALL.get(i).copied().unwrap_or_default()
    }

    pub fn next(self) -> Self {
        Tab::from_index((self.index() + 1) % Tab::ALL.len())
    }

    pub fn prev(self) -> Self {
        Tab::from_index((self.index() + Tab::ALL.len() - 1) % Tab::ALL.len())
    }
}

/// Sort direction for sortable tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

impl SortDir {
    pub fn toggle(self) -> Self {
        match self {
            SortDir::Asc  => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }
}

/// Full UI state — current tab, per-table sort column (by index), direction, and row selection.
#[derive(Debug)]
pub struct TuiState {
    pub tab: Tab,

    /// Jobs table: col 0=Age, 1=Client, 2=Worker, 3=State, 4=Queue time, 5=Wkr time, 6=ID
    pub jobs_sort_col: usize,
    pub jobs_sort_dir: SortDir,
    pub jobs_selected: usize,

    /// Workers table: col 0=Address, 1=Status, 2=Jobs rcvd, 3=Avg job, 4=Total time, 5=Connected
    pub workers_sort_col: usize,
    pub workers_sort_dir: SortDir,
    pub workers_selected: usize,

    /// Clients table: col 0=Address, 1=Jobs, 2=Avg queue, 3=Tot queue, 4=Avg worker, 5=Tot worker, 6=Connected
    pub clients_sort_col: usize,
    pub clients_sort_dir: SortDir,
    pub clients_selected: usize,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            tab: Tab::default(),
            // Sort jobs by Age (col 0) descending = youngest first
            jobs_sort_col: 0,
            jobs_sort_dir: SortDir::Desc,
            jobs_selected: 0,
            workers_sort_col: 0,
            workers_sort_dir: SortDir::default(),
            workers_selected: 0,
            clients_sort_col: 0,
            clients_sort_dir: SortDir::default(),
            clients_selected: 0,
        }
    }
}
