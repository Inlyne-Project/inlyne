use std::str::FromStr;

use crate::keybindings::action::HistDirection;

use super::action::{Action, VertDirection, Zoom};
use super::{Key, KeyCombo, ModifiedKey};

use serde::{de, Deserialize, Deserializer};
use winit::event::{ModifiersState, VirtualKeyCode as VirtKey};

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        enum FlatAction {
            HistoryNext,
            HistoryPrevious,
            ToTop,
            ToBottom,
            ScrollUp,
            ScrollDown,
            PageUp,
            PageDown,
            ZoomIn,
            ZoomOut,
            ZoomReset,
            Copy,
            Quit,
        }

        let action = match FlatAction::deserialize(deserializer)? {
            FlatAction::HistoryNext => Action::History(HistDirection::Next),
            FlatAction::HistoryPrevious => Action::History(HistDirection::Prev),
            FlatAction::ToTop => Action::ToEdge(VertDirection::Up),
            FlatAction::ToBottom => Action::ToEdge(VertDirection::Down),
            FlatAction::ScrollUp => Action::Scroll(VertDirection::Up),
            FlatAction::ScrollDown => Action::Scroll(VertDirection::Down),
            FlatAction::PageUp => Action::Page(VertDirection::Up),
            FlatAction::PageDown => Action::Page(VertDirection::Down),
            FlatAction::ZoomIn => Action::Zoom(Zoom::In),
            FlatAction::ZoomOut => Action::Zoom(Zoom::Out),
            FlatAction::ZoomReset => Action::Zoom(Zoom::Reset),
            FlatAction::Copy => Action::Copy,
            FlatAction::Quit => Action::Quit,
        };

        Ok(action)
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrNum {
            Str(String),
            Num(u32),
        }

        match StringOrNum::deserialize(deserializer)? {
            StringOrNum::Str(s) => Key::from_str(&s).map_err(de::Error::custom),
            StringOrNum::Num(num) => Ok(Self::ScanCode(num)),
        }
    }
}

struct ShortKey {
    key: Key,
    shift: bool,
}

impl<'de> Deserialize<'de> for ShortKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrNum {
            Str(String),
            Num(u32),
        }

        let shifted_key = |virt_key| {
            Ok(Self {
                key: Key::Resolved(virt_key),
                shift: true,
            })
        };

        match StringOrNum::deserialize(deserializer)? {
            StringOrNum::Str(s) => match &*s {
                "A" => shifted_key(VirtKey::A),
                "B" => shifted_key(VirtKey::B),
                "C" => shifted_key(VirtKey::C),
                "D" => shifted_key(VirtKey::D),
                "E" => shifted_key(VirtKey::E),
                "F" => shifted_key(VirtKey::F),
                "G" => shifted_key(VirtKey::G),
                "H" => shifted_key(VirtKey::H),
                "I" => shifted_key(VirtKey::I),
                "J" => shifted_key(VirtKey::J),
                "K" => shifted_key(VirtKey::K),
                "L" => shifted_key(VirtKey::L),
                "M" => shifted_key(VirtKey::M),
                "N" => shifted_key(VirtKey::N),
                "O" => shifted_key(VirtKey::O),
                "P" => shifted_key(VirtKey::P),
                "Q" => shifted_key(VirtKey::Q),
                "R" => shifted_key(VirtKey::R),
                "S" => shifted_key(VirtKey::S),
                "T" => shifted_key(VirtKey::T),
                "U" => shifted_key(VirtKey::U),
                "V" => shifted_key(VirtKey::V),
                "W" => shifted_key(VirtKey::W),
                "X" => shifted_key(VirtKey::X),
                "Y" => shifted_key(VirtKey::Y),
                "Z" => shifted_key(VirtKey::Z),
                other => match Key::from_str(other) {
                    Ok(key) => Ok(Self { key, shift: false }),
                    Err(err) => Err(de::Error::custom(err)),
                },
            },
            StringOrNum::Num(num) => Ok(Self {
                key: Key::ScanCode(num),
                shift: false,
            }),
        }
    }
}

impl<'de> Deserialize<'de> for ModifiedKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        enum ModifierType {
            Alt,
            Ctrl,
            Os,
            Shift,
        }

        #[derive(Deserialize)]
        struct Inner {
            key: ShortKey,
            r#mod: Vec<ModifierType>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum KeyOrModifiedKey {
            Key(ShortKey),
            ModifiedKey(Inner),
        }

        Ok(match KeyOrModifiedKey::deserialize(deserializer)? {
            KeyOrModifiedKey::Key(ShortKey { key, shift }) => {
                let mut mods = ModifiersState::empty();
                if shift {
                    mods |= ModifiersState::SHIFT;
                }
                ModifiedKey(key, mods)
            }
            KeyOrModifiedKey::ModifiedKey(Inner {
                key: ShortKey { key, shift },
                r#mod,
            }) => {
                let mut modifiers = ModifiersState::empty();
                for ty in r#mod {
                    modifiers |= match ty {
                        ModifierType::Alt => ModifiersState::ALT,
                        ModifierType::Ctrl => ModifiersState::CTRL,
                        ModifierType::Os => ModifiersState::LOGO,
                        ModifierType::Shift => ModifiersState::SHIFT,
                    };
                }
                if shift {
                    modifiers |= ModifiersState::SHIFT;
                }

                ModifiedKey(key, modifiers)
            }
        })
    }
}

impl<'de> Deserialize<'de> for KeyCombo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ModifiedKeyOrModifiedKeys {
            Key(ModifiedKey),
            Keys(Vec<ModifiedKey>),
        }

        let keys = match ModifiedKeyOrModifiedKeys::deserialize(deserializer)? {
            ModifiedKeyOrModifiedKeys::Key(key) => vec![key],
            ModifiedKeyOrModifiedKeys::Keys(keys) => keys,
        };

        Ok(KeyCombo(keys))
    }
}
