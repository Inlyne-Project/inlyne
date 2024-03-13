use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct History {
    history: Vec<PathBuf>,
    index: usize,
}

impl History {
    pub fn new(path_buf: PathBuf) -> Self {
        Self {
            history: vec![path_buf],
            index: 0,
        }
    }

    pub fn get_path(&self) -> &Path {
        self.history
            .get(self.index)
            .expect("History should always be in bounds")
            .as_path()
    }

    pub fn make_next(&mut self, file_path: PathBuf) {
        self.history.truncate(self.index + 1);
        self.history.push(file_path);
        self.index += 1;
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&Path> {
        if self.index + 1 == self.history.len() {
            None
        } else {
            self.index += 1;
            Some(self.get_path())
        }
    }

    pub fn previous(&mut self) -> Option<&Path> {
        if self.index == 0 {
            None
        } else {
            self.index -= 1;
            Some(self.get_path())
        }
    }
}
