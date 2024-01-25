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

use winit::event::{ModifiersState, ScanCode, VirtualKeyCode as VirtKey};

use action::Action;
pub use keybindings::Keybindings;

use crate::opts::KeybindingsSection;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Resolved(VirtKey),
    ScanCode(ScanCode),
}

impl Key {
    pub fn new(resolved: Option<VirtKey>, scan_code: ScanCode) -> Self {
        match resolved {
            Some(key_code) => Self::Resolved(key_code),
            None => Self::ScanCode(scan_code),
        }
    }
}

impl From<VirtKey> for Key {
    fn from(key_code: VirtKey) -> Self {
        Self::Resolved(key_code)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Resolved(resolved) => {
                let maybe_key = mappings::STR_TO_VIRT_KEY
                    .iter()
                    .find_map(|&(key_str, key)| (*resolved == key).then_some(key_str));
                match maybe_key {
                    Some(key) => f.write_str(key),
                    None => write!(f, "<unsupported: {resolved:?}>"),
                }
            }
            Key::ScanCode(scan_code) => write!(f, "<scan code: {scan_code}>"),
        }
    }
}

impl FromStr for Key {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        mappings::STR_TO_VIRT_KEY
            .iter()
            .find_map(|&(key_str, key)| (s == key_str).then_some(Key::Resolved(key)))
            .ok_or_else(|| anyhow::anyhow!("Unsupported key: {s}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModifiedKey(pub Key, pub ModifiersState);

impl fmt::Display for ModifiedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.1 == ModifiersState::default() {
            let is_not_visible = [
                VirtKey::F1,
                VirtKey::F2,
                VirtKey::F3,
                VirtKey::F4,
                VirtKey::F5,
                VirtKey::F6,
                VirtKey::F7,
                VirtKey::F8,
                VirtKey::F9,
                VirtKey::F10,
                VirtKey::F11,
                VirtKey::F12,
                VirtKey::Up,
                VirtKey::Right,
                VirtKey::Down,
                VirtKey::Left,
                VirtKey::Escape,
                VirtKey::Tab,
                VirtKey::Insert,
                VirtKey::Delete,
                VirtKey::Back,
                VirtKey::Return,
                VirtKey::Home,
                VirtKey::End,
                VirtKey::PageUp,
                VirtKey::PageDown,
                VirtKey::Space,
            ]
            .map(Key::from)
            .contains(&self.0);

            if is_not_visible {
                write!(f, "<{}>", self.0)
            } else {
                write!(f, "{}", self.0)
            }
        } else {
            let mut mod_list = Vec::new();

            if self.1.alt() {
                mod_list.push("Alt");
            }
            if self.1.ctrl() {
                mod_list.push("Ctrl");
            }
            if self.1.logo() {
                mod_list.push("Os");
            }
            if self.1.shift() {
                mod_list.push("Shift");
            }

            let mods = mod_list.join("+");
            write!(f, "<{}+{}>", mods, self.0)
        }
    }
}

impl From<VirtKey> for ModifiedKey {
    fn from(keycode: VirtKey) -> Self {
        Self(Key::from(keycode), ModifiersState::empty())
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

impl From<VirtKey> for KeyCombo {
    fn from(key_code: VirtKey) -> Self {
        KeyCombo(vec![ModifiedKey::from(key_code)])
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
        if let Key::Resolved(key) = &modified_key.0 {
            if [
                VirtKey::LAlt,
                VirtKey::RAlt,
                VirtKey::LControl,
                VirtKey::RControl,
                VirtKey::LWin,
                VirtKey::RWin,
                VirtKey::LShift,
                VirtKey::RShift,
            ]
            .contains(key)
            {
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
