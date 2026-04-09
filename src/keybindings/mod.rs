pub mod action;
mod defaults;
#[allow(clippy::module_inception)]
mod keybindings;
mod mappings;
mod serialization;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::slice::Iter;
use std::str::FromStr;
use std::vec;

use winit::keyboard::{ModifiersState, NamedKey, PhysicalKey, KeyCode};

use action::Action;
pub use keybindings::Keybindings;

use crate::opts::KeybindingsSection;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Named(NamedKey),
    Character(String),
    KeyCode(KeyCode),
}

impl Key {
    pub fn from_named(named: NamedKey) -> Self {
        Self::Named(named)
    }

    pub fn from_character(c: impl Into<String>) -> Self {
        Self::Character(c.into())
    }

    pub fn from_winit_key(
        logical_key: &winit::keyboard::Key,
        physical_key: &PhysicalKey,
    ) -> Self {
        match logical_key {
            winit::keyboard::Key::Named(named) => Key::Named(named.clone()),
            winit::keyboard::Key::Character(c) => Key::Character(c.to_string()),
            _ => {
                if let PhysicalKey::Code(code) = physical_key {
                    Key::KeyCode(*code)
                } else {
                    Key::Character(String::new())
                }
            }
        }
    }
}

impl From<NamedKey> for Key {
    fn from(named: NamedKey) -> Self {
        Self::Named(named)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Named(named) => {
                let mapped = mappings::MappedKey::Named(named.clone());
                match mappings::mapped_key_to_str(&mapped) {
                    Some(s) => f.write_str(s),
                    None => write!(f, "<unsupported: {named:?}>"),
                }
            }
            Key::Character(c) => f.write_str(c),
            Key::KeyCode(code) => write!(f, "<key code: {code:?}>"),
        }
    }
}

impl FromStr for Key {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match mappings::str_to_mapped_key(s) {
            Some(mappings::MappedKey::Named(named)) => Ok(Key::Named(named)),
            Some(mappings::MappedKey::Character(c)) => Ok(Key::Character(c)),
            None => Err(anyhow::anyhow!("Unsupported key: {s}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModifiedKey(pub Key, pub ModifiersState);

impl fmt::Display for ModifiedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.1 == ModifiersState::empty() {
            let is_not_visible = matches!(
                &self.0,
                Key::Named(
                    NamedKey::F1 | NamedKey::F2 | NamedKey::F3 | NamedKey::F4 |
                    NamedKey::F5 | NamedKey::F6 | NamedKey::F7 | NamedKey::F8 |
                    NamedKey::F9 | NamedKey::F10 | NamedKey::F11 | NamedKey::F12 |
                    NamedKey::ArrowUp | NamedKey::ArrowRight | NamedKey::ArrowDown | NamedKey::ArrowLeft |
                    NamedKey::Escape | NamedKey::Tab | NamedKey::Insert | NamedKey::Delete |
                    NamedKey::Backspace | NamedKey::Enter | NamedKey::Home | NamedKey::End |
                    NamedKey::PageUp | NamedKey::PageDown | NamedKey::Space
                )
            );

            if is_not_visible {
                write!(f, "<{}>", self.0)
            } else {
                write!(f, "{}", self.0)
            }
        } else {
            let mut mod_list = Vec::new();

            if self.1.alt_key() {
                mod_list.push("Alt");
            }
            if self.1.control_key() {
                mod_list.push("Ctrl");
            }
            if self.1.super_key() {
                mod_list.push("Os");
            }
            if self.1.shift_key() {
                mod_list.push("Shift");
            }

            let mods = mod_list.join("+");
            write!(f, "<{}+{}>", mods, self.0)
        }
    }
}

impl From<NamedKey> for ModifiedKey {
    fn from(named: NamedKey) -> Self {
        Self(Key::from(named), ModifiersState::empty())
    }
}

impl From<Key> for ModifiedKey {
    fn from(key: Key) -> Self {
        Self(key, ModifiersState::empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyCombo(pub Vec<ModifiedKey>);

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for key in self.iter() {
            write!(f, "{key}")?;
        }

        Ok(())
    }
}

impl KeyCombo {
    fn iter(&self) -> Iter<'_, ModifiedKey> {
        self.0.iter()
    }

    fn into_iter(self) -> vec::IntoIter<ModifiedKey> {
        self.0.into_iter()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn starts_with(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }
}

impl From<NamedKey> for KeyCombo {
    fn from(named: NamedKey) -> Self {
        KeyCombo(vec![ModifiedKey::from(named)])
    }
}

impl From<Key> for KeyCombo {
    fn from(key: Key) -> Self {
        KeyCombo(vec![ModifiedKey::from(key)])
    }
}

type Node = BTreeMap<ModifiedKey, Connection>;
type Ptr = usize;
const ROOT_INDEX: Ptr = 0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Connection {
    Branch(Ptr),
    Leaf(Action),
}

/// Maps single or multi key combos to their actions
///
/// Internally this is implemented as a trie (a tree where prefixes are shared) where the
/// "pointers" are all just indices into `storage`. Each entry in `storage` represents a node with
/// its connections to other nodes stored in a map
#[derive(Debug, Default)]
pub struct KeyCombos {
    position: Ptr,
    storage: Vec<Node>,
    in_multikey_combo: bool,
}

impl KeyCombos {
    pub fn new(keybinds: KeybindingsSection) -> anyhow::Result<Self> {
        let keybinds: Keybindings = keybinds.into();
        let position = ROOT_INDEX;

        // A keycombo that starts with another keycombo will never be reachable since the prefixing
        // combo will always be activated first
        for (i, (_, combo1)) in keybinds.iter().enumerate() {
            for (_, combo2) in keybinds.iter().skip(i + 1) {
                if combo1.starts_with(combo2) {
                    anyhow::bail!(
                        "A keycombo starts with another keycombo making it unreachable\n\tCombo: \
                            {combo1}\n\tPrefix: {combo2}"
                    );
                } else if combo2.starts_with(combo1) {
                    anyhow::bail!(
                        "A keycombo starts with another keycombo making it unreachable\n\tCombo: \
                            {combo2}\n\tPrefix: {combo1}"
                    );
                }
            }
        }

        let mut storage = Vec::new();
        for (action, keys) in keybinds {
            anyhow::ensure!(
                !keys.is_empty(),
                "A keycombo for {action:?} contained no keys"
            );
            Self::insert_action(&mut storage, keys, action);
        }

        Ok(Self {
            position,
            storage,
            in_multikey_combo: false,
        })
    }

    fn insert_action(storage: &mut Vec<Node>, keys: KeyCombo, action: Action) {
        Self::insert_action_(storage, keys.into_iter(), action, ROOT_INDEX)
    }

    fn insert_action_(
        storage: &mut Vec<Node>,
        mut keys: vec::IntoIter<ModifiedKey>,
        action: Action,
        position: Ptr,
    ) {
        let key = keys.next().unwrap();

        // We're on a new connection, push a node for it
        if storage.len() <= position {
            storage.push(BTreeMap::new());
        }
        let next_free_position = storage.len();
        let node = storage.get_mut(position).unwrap();
        let value = node.get(&key).cloned();

        match value {
            Some(Connection::Branch(common_branch)) => {
                assert_ne!(keys.len(), 0, "Prefixes are checked before inserting");
                Self::insert_action_(storage, keys, action, common_branch);
            }
            Some(Connection::Leaf(_)) => unreachable!("Prefixes are checked before inserting"),
            None => {
                if keys.len() == 0 {
                    let _ = node.insert(key, Connection::Leaf(action));
                } else {
                    let _ = node.insert(key, Connection::Branch(next_free_position));
                    Self::insert_action_(storage, keys, action, next_free_position);
                }
            }
        }
    }

    /// Processes a modified key and emits the corresponding action if this completes a keycombo
    pub fn munch(&mut self, modified_key: ModifiedKey) -> Option<Action> {
        // We ignore modifier keys since they aren't considered part of combos
        if let Key::Named(named) = &modified_key.0 {
            if matches!(
                named,
                NamedKey::Alt | NamedKey::Control | NamedKey::Super | NamedKey::Shift
            ) {
                return None;
            }
        }

        tracing::debug!("Received key: {modified_key}");

        let maybe_action = self.munch_(modified_key);

        if let Some(action) = maybe_action {
            tracing::debug!("Emitting action: {:?}", action);
        }

        maybe_action
    }

    fn munch_(&mut self, modified_key: ModifiedKey) -> Option<Action> {
        let node = self.storage.get(self.position)?;

        match node.get(&modified_key) {
            Some(&Connection::Leaf(action)) => {
                self.reset();
                Some(action)
            }
            Some(&Connection::Branch(next_position)) => {
                self.in_multikey_combo = true;
                self.position = next_position;
                None
            }
            None => {
                let in_multikey_combo = self.in_multikey_combo;
                self.reset();
                if in_multikey_combo {
                    // If we were broken out of a multi-key combo the key that broke us out could be
                    // part of a new keycombo
                    self.munch_(modified_key)
                } else {
                    None
                }
            }
        }
    }

    fn reset(&mut self) {
        // Wipe everything, but the nodes
        self.position = ROOT_INDEX;
        self.in_multikey_combo = false;
    }
}
