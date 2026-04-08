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
            Tab::Jobs => "Jobs",
            Tab::Workers => "Workers",
            Tab::Clients => "Clients",
            Tab::Logs => "Logs",
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
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }
}

/// Sort column for the Jobs table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobSortCol {
    #[default]
    QueuedAt,
    State,
    QueueTime,
    WorkerTime,
    Client,
    Worker,
}

impl JobSortCol {
    pub const ALL: &'static [JobSortCol] = &[
        JobSortCol::QueuedAt,
        JobSortCol::State,
        JobSortCol::QueueTime,
        JobSortCol::WorkerTime,
        JobSortCol::Client,
        JobSortCol::Worker,
    ];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Sort column for the Workers table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkerSortCol {
    #[default]
    ConnectedAt,
    JobsReceived,
    TotalJobTime,
    Status,
}

impl WorkerSortCol {
    pub const ALL: &'static [WorkerSortCol] = &[
        WorkerSortCol::ConnectedAt,
        WorkerSortCol::JobsReceived,
        WorkerSortCol::TotalJobTime,
        WorkerSortCol::Status,
    ];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Sort column for the Clients table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClientSortCol {
    #[default]
    ConnectedAt,
    JobsSubmitted,
    TotalQueueTime,
    TotalWorkerTime,
}

impl ClientSortCol {
    pub const ALL: &'static [ClientSortCol] = &[
        ClientSortCol::ConnectedAt,
        ClientSortCol::JobsSubmitted,
        ClientSortCol::TotalQueueTime,
        ClientSortCol::TotalWorkerTime,
    ];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Full UI state for the TUI — current tab, sort settings, and row selections.
#[derive(Debug, Default)]
pub struct TuiState {
    pub tab: Tab,

    pub jobs_sort_col: JobSortCol,
    pub jobs_sort_dir: SortDir,
    pub jobs_selected: usize,

    pub workers_sort_col: WorkerSortCol,
    pub workers_sort_dir: SortDir,
    pub workers_selected: usize,

    pub clients_sort_col: ClientSortCol,
    pub clients_sort_dir: SortDir,
    pub clients_selected: usize,
}
