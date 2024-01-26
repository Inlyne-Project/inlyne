use serde::Deserialize;

use crate::opts::KeybindingsSection;

use super::{action::Action, KeyCombo};

/// A list of [`keybindings`](KeyCombo) each associated with an [`Action`].
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Keybindings(Vec<(Action, KeyCombo)>);

impl Keybindings {
    /// Returns an iterator over the [`Action`]s and [`KeyCombo`]s
    pub fn iter(&self) -> std::slice::Iter<'_, (Action, KeyCombo)> {
        self.0.iter()
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

impl From<KeybindingsSection> for Keybindings {
    /// Converts from [`KeybindingsSection`] to [`Keybindings`].
    ///
    /// If an `extra` keybinding collides with a `base` one, then the `base` one is dropped in
    /// favor of the `extra` keybinding
    fn from(value: KeybindingsSection) -> Self {
        let mut base = value.base;

        if let Some(extra) = value.extra {
            for (_, extra_combo) in extra.iter() {
                base.0 = base
                    .clone()
                    .into_iter()
                    .filter(|(_, combo)| !combo.starts_with(extra_combo))
                    .collect();
            }

            base.extend(extra)
        }

        base
    }
}

#[cfg(test)]
mod tests {
    use winit::event::ModifiersState;

    use crate::keybindings::{action::VertDirection, Key, ModifiedKey};

    use super::*;

    #[test]
    fn from_keybinding_section_base() {
        assert_eq!(
            Keybindings::from(KeybindingsSection {
                base: Keybindings::default(),
                extra: None
            }),
            Keybindings::default()
        );
    }

    #[test]
    fn from_keybinding_section_extra() {
        let combo = KeyCombo(vec![ModifiedKey(
            Key::Resolved(winit::event::VirtualKeyCode::A),
            ModifiersState::empty(),
        )]);

        let mut expected = Keybindings::default();
        expected.0.push((Action::Quit, combo.clone()));

        assert_eq!(
            Keybindings::from(KeybindingsSection {
                base: Keybindings::default(),
                extra: Some(Keybindings(vec![(Action::Quit, combo)]))
            }),
            expected
        );
    }

    #[test]
    fn from_keybinding_section_extra_override_base() {
        let j_combo = KeyCombo(vec![ModifiedKey(
            Key::Resolved(winit::event::VirtualKeyCode::J),
            ModifiersState::empty(),
        )]);

        let base = Keybindings(vec![(Action::Scroll(VertDirection::Down), j_combo.clone())]);
        let extra = Keybindings(vec![(Action::Page(VertDirection::Down), j_combo.clone())]);

        let expected = Keybindings(vec![(Action::Page(VertDirection::Down), j_combo.clone())]);

        assert_eq!(
            Keybindings::from(KeybindingsSection {
                base,
                extra: Some(extra)
            }),
            expected
        );
    }
}
