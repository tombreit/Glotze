use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Progress {
    pub id: u64,
    pub title: String,
    pub state: State,
}

#[derive(Debug, Clone)]
pub enum State {
    Running { bytes_done: u64, bytes_total: u64 },
    Done { path: PathBuf },
    Failed { reason: String },
    Cancelled,
}

impl State {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, State::Running { .. })
    }
}
