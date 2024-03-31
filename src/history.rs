use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct History {
    history: Vec<PathBuf>,
    index: usize,
}

impl History {
    pub fn new(path_buf: PathBuf) -> Self {
        #[cfg(not(test))]
        let path_buf = path_buf.canonicalize().unwrap();
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
        #[cfg(not(test))]
        let file_path = file_path.canonicalize().unwrap();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanity() {
        let root = PathBuf::from("a");
        let fork1 = PathBuf::from("b");
        let fork2 = PathBuf::from("c");

        let mut hist = History::new(root.clone());
        assert_eq!(hist.get_path(), root);
        assert_eq!(hist.previous(), None);

        hist.make_next(fork1.clone());
        assert_eq!(hist.get_path(), fork1);

        assert_eq!(hist.previous().unwrap(), root);
        hist.make_next(fork2.clone());
        assert_eq!(hist.get_path(), fork2);

        assert_eq!(hist.previous().unwrap(), root);
        assert_eq!(hist.previous(), None);
        assert_eq!(hist.next().unwrap(), fork2);
        assert_eq!(hist.next(), None);
    }
}
