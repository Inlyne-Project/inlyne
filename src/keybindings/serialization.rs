use std::str::FromStr;

use crate::keybindings::action::HistDirection;

use super::action::{Action, VertDirection, Zoom};
use super::{Key, KeyCombo, ModifiedKey};

use serde::{de, Deserialize, Deserializer};
use winit::keyboard::ModifiersState;

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
            StringOrNum::Num(num) => {
                // Legacy scan code support - map to a key code if possible
                // For backwards compat, just store as a character representation
                Ok(Self::Character(format!("<scancode:{num}>")))
            }
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

        let shifted_key = |c: &str| {
            Ok(Self {
                key: Key::Character(c.to_lowercase()),
                shift: true,
            })
        };

        match StringOrNum::deserialize(deserializer)? {
            StringOrNum::Str(s) => match &*s {
                "A" => shifted_key("a"),
                "B" => shifted_key("b"),
                "C" => shifted_key("c"),
                "D" => shifted_key("d"),
                "E" => shifted_key("e"),
                "F" => shifted_key("f"),
                "G" => shifted_key("g"),
                "H" => shifted_key("h"),
                "I" => shifted_key("i"),
                "J" => shifted_key("j"),
                "K" => shifted_key("k"),
                "L" => shifted_key("l"),
                "M" => shifted_key("m"),
                "N" => shifted_key("n"),
                "O" => shifted_key("o"),
                "P" => shifted_key("p"),
                "Q" => shifted_key("q"),
                "R" => shifted_key("r"),
                "S" => shifted_key("s"),
                "T" => shifted_key("t"),
                "U" => shifted_key("u"),
                "V" => shifted_key("v"),
                "W" => shifted_key("w"),
                "X" => shifted_key("x"),
                "Y" => shifted_key("y"),
                "Z" => shifted_key("z"),
                other => match Key::from_str(other) {
                    Ok(key) => Ok(Self { key, shift: false }),
                    Err(err) => Err(de::Error::custom(err)),
                },
            },
            StringOrNum::Num(num) => Ok(Self {
                key: Key::Character(format!("<scancode:{num}>")),
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
        #[serde(untagged)]
        enum ModOrMods {
            Mod(ModifierType),
            Mods(Vec<ModifierType>),
        }

        #[derive(Deserialize)]
        struct Inner {
            key: ShortKey,
            #[serde(rename = "mod")]
            mod_: ModOrMods,
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
                mod_,
            }) => {
                let mut modifiers = ModifiersState::empty();
                let mod_ = match mod_ {
                    ModOrMods::Mod(m) => vec![m],
                    ModOrMods::Mods(mods) => mods,
                };
                for ty in mod_ {
                    modifiers |= match ty {
                        ModifierType::Alt => ModifiersState::ALT,
                        ModifierType::Ctrl => ModifiersState::CONTROL,
                        ModifierType::Os => ModifiersState::SUPER,
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
