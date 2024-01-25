use serde::Deserialize;

use super::{action::Action, KeyCombo};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Keybindings(Vec<(Action, KeyCombo)>);

impl Keybindings {
    pub fn new(bindings: Vec<(Action, KeyCombo)>) -> Self {
        Self(bindings)
    }

    #[cfg(test)]
    pub fn empty() -> Self {
        Self(Vec::new())
    }
}

impl Extend<(Action, KeyCombo)> for Keybindings {
    fn extend<I: IntoIterator<Item = (Action, KeyCombo)>>(&mut self, iter: I) {
        self.0.extend(iter)
    }
}

impl IntoIterator for Keybindings {
    type Item = (Action, KeyCombo);
    type IntoIter = <Vec<(Action, KeyCombo)> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Default for Keybindings {
    fn default() -> Self {
        Self(super::defaults::defaults())
    }
}
