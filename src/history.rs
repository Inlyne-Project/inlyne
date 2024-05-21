use std::path::{Path, PathBuf};

use anyhow::Context;

#[derive(Debug, Clone, PartialEq)]
pub struct History {
    history: Vec<PathBuf>,
    index: usize,
}

impl History {
    pub fn new(path: &Path) -> anyhow::Result<Self> {
        let canonicalized = path
            .canonicalize()
            .with_context(|| format!("Unable to canonicalize {}", path.display()))?;
        Ok(Self {
            history: vec![canonicalized],
            index: 0,
        })
    }

    pub fn get_path(&self) -> &Path {
        self.history
            .get(self.index)
            .expect("History should always be in bounds")
            .as_path()
    }

    pub fn make_next(&mut self, file_path: PathBuf) {
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
    use std::fs;

    use super::*;

    #[test]
    fn sanity() {
        let temp_dir = tempfile::Builder::new()
            .prefix("inlyne-tests-")
            .tempdir()
            .unwrap();
        let temp_path = temp_dir.path().canonicalize().unwrap();

        let root = temp_path.join("a");
        let fork1 = temp_path.join("b");
        let fork2 = temp_path.join("c");
        fs::write(&root, "a").unwrap();
        fs::write(&fork1, "b").unwrap();
        fs::write(&fork2, "c").unwrap();

        let mut hist = History::new(&root).unwrap();
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
