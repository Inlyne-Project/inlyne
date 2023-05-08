mod defaults;
mod mappings;
mod serialization;
#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, fmt, slice::Iter, str::FromStr, vec::IntoIter};

use serde::Deserialize;
use winit::event::{ModifiersState, ScanCode, VirtualKeyCode};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Keybindings(Vec<(Action, KeyCombo)>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Resolved(VirtualKeyCode),
    ScanCode(ScanCode),
}

impl Keybindings {
    #[inline]
    pub fn new(bindings: Vec<(Action, KeyCombo)>) -> Self {
        Self(bindings)
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

impl Key {
    pub fn new(resolved: Option<VirtualKeyCode>, scan_code: ScanCode) -> Self {
        match resolved {
            Some(key_code) => Self::Resolved(key_code),
            None => Self::ScanCode(scan_code),
        }
    }
}

impl From<VirtualKeyCode> for Key {
    fn from(key_code: VirtualKeyCode) -> Self {
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
                    None => write!(f, "<unsupported: {:?}>", resolved),
                }
            }
            Key::ScanCode(_) => write!(f, "{:?}", self),
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
            write!(f, "{}", self.0)
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

            let mods = mod_list.join(", ");
            write!(f, "{{ {}, [{mods}] }}", self.0)
        }
    }
}

impl From<VirtualKeyCode> for ModifiedKey {
    fn from(keycode: VirtualKeyCode) -> Self {
        Self(Key::from(keycode), ModifiersState::empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyCombo(pub Vec<ModifiedKey>);

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys = self
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "[{}]", keys)
    }
}

impl KeyCombo {
    fn iter(&self) -> Iter<'_, ModifiedKey> {
        self.0.iter()
    }

    fn into_iter(self) -> IntoIter<ModifiedKey> {
        self.0.into_iter()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn starts_with(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }
}

impl From<VirtualKeyCode> for KeyCombo {
    fn from(key_code: VirtualKeyCode) -> Self {
        KeyCombo(vec![ModifiedKey::from(key_code)])
    }
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ToTop,
    ToBottom,
    ScrollUp,
    ScrollDown,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Copy,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Connection {
    Branch(usize),
    Leaf(Action),
}

const ROOT_INDEX: usize = 0;

/// Maps single or multi key combos to their actions
///
/// Internally this is implemented as a trie (a tree where prefixes are shared) where the
/// "pointers" are all just indices into `storage`. Each entry in storage represents a node with
/// its connections to other nodes stored in a map
#[derive(Debug)]
pub struct KeyCombos {
    position: usize,
    storage: Vec<BTreeMap<ModifiedKey, Connection>>,
    in_multikey_combo: bool,
}

impl KeyCombos {
    pub fn new(keybinds: Keybindings) -> anyhow::Result<Self> {
        let keybinds = keybinds.0;
        let position = ROOT_INDEX;

        // A keycombo that starts with another keycombo will never be reachable since the prefixing
        // combo will always be activated first
        for i in 0..keybinds.len() {
            for j in (i + 1)..keybinds.len() {
                let combo1 = &keybinds[i].1;
                let combo2 = &keybinds[j].1;
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
            let keys = keys.into_iter();
            Self::insert_action(&mut storage, ROOT_INDEX, keys, action);
        }

        Ok(Self {
            position,
            storage,
            in_multikey_combo: false,
        })
    }

    fn insert_action(
        storage: &mut Vec<BTreeMap<ModifiedKey, Connection>>,
        position: usize,
        mut keys: IntoIter<ModifiedKey>,
        action: Action,
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
                if keys.len() == 0 {
                    unreachable!("Prefixes are checked before inserting");
                } else {
                    Self::insert_action(storage, common_branch, keys, action);
                }
            }
            Some(Connection::Leaf(_)) => unreachable!("Prefixes are checked before inserting"),
            None => {
                if keys.len() == 0 {
                    let _ = node.insert(key, Connection::Leaf(action));
                } else {
                    let _ = node.insert(key, Connection::Branch(next_free_position));
                    Self::insert_action(storage, next_free_position, keys, action);
                }
            }
        }
    }

    /// Processes a modified key and emits the corresponding action if this completes a keycombo
    pub fn munch(&mut self, modified_key: ModifiedKey) -> Option<Action> {
        // We ignore modifier keys since they aren't considered part of combos
        if let Key::Resolved(key) = &modified_key.0 {
            if [
                VirtualKeyCode::LAlt,
                VirtualKeyCode::RAlt,
                VirtualKeyCode::LControl,
                VirtualKeyCode::RControl,
                VirtualKeyCode::LWin,
                VirtualKeyCode::RWin,
                VirtualKeyCode::LShift,
                VirtualKeyCode::RShift,
            ]
            .contains(key)
            {
                return None;
            }
        }

        log::debug!("Recieved key: {modified_key}");

        let maybe_action = self.munch_(modified_key);

        if let Some(action) = maybe_action {
            log::debug!("Emitting action: {:?}", action);
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
                // If we were broken out of a multi-key combo the key that broke us out could be
                // part of a new keycombo. In that case reset the combo and run it
                if self.in_multikey_combo {
                    self.reset();
                    self.munch_(modified_key)
                } else {
                    self.reset();
                    None
                }
            }
        }
    }

    fn reset(&mut self) {
        self.position = ROOT_INDEX;
        self.in_multikey_combo = false;
    }
}
