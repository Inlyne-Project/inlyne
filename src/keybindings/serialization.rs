use std::str::FromStr;

use super::{Key, KeyCombo, ModifiedKey};

use serde::{de, Deserialize, Deserializer};
use winit::event::ModifiersState;

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
            key: Key,
            r#mod: Vec<ModifierType>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum KeyOrModifiedKey {
            Key(Key),
            ModifiedKey(Inner),
        }

        Ok(match KeyOrModifiedKey::deserialize(deserializer)? {
            KeyOrModifiedKey::Key(key) => ModifiedKey(key, ModifiersState::empty()),
            KeyOrModifiedKey::ModifiedKey(Inner { key, r#mod }) => {
                let mut modifiers = ModifiersState::empty();
                for ty in r#mod {
                    modifiers |= match ty {
                        ModifierType::Alt => ModifiersState::ALT,
                        ModifierType::Ctrl => ModifiersState::CTRL,
                        ModifierType::Os => ModifiersState::LOGO,
                        ModifierType::Shift => ModifiersState::SHIFT,
                    };
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
