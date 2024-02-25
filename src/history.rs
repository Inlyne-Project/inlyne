use smart_debug::SmartDebug;
use std::path::PathBuf;

#[derive(SmartDebug, Clone, PartialEq)]
pub struct History {
    history: Vec<PathBuf>,
    index: usize,
}

impl History {
    pub fn new(path_buf: PathBuf) -> Self {
        History {
            history: vec![path_buf],
            index: 0,
        }
    }
    pub fn truncate(&mut self) {
        if self.index + 1 < self.history.len() {
            self.history.truncate(self.index + 1);
        }
    }
    pub fn append(&mut self, file_path: PathBuf) {
        self.history.push(file_path);
        self.index += 1;
    }
    pub fn get_path(&self) -> &PathBuf {
        self.history
            .get(self.index)
            .expect("History should be bound checked for all possible indexes.")
    }

    pub fn next(&mut self) -> Option<&PathBuf> {
        if self.index + 1 == self.history.len() {
            return None;
        }
        self.index += 1;
        Some(self.get_path())
    }
    pub fn previous(&mut self) -> Option<&PathBuf> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        Some(self.get_path())
    }
}
