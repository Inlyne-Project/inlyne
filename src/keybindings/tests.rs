use super::{Action, Key, KeyCombo, KeyCombos, ModifiedKey, Keybindings};

use serde::Deserialize;
use winit::event::{ModifiersState, VirtualKeyCode};

#[test]
fn sanity() {
    #[derive(Deserialize, Debug)]
    struct Holder {
        inner: Vec<(Action, KeyCombo)>,
    }

    let slim_config = r#"
inner = [
    ["ToTop", ["g", "g"]],
    ["ToBottom", { key = "g", mod = ["Shift"] }],
    ["ScrollDown", ["g", "j"]],
    ["ScrollDown", "j"],
]
"#;

    let Holder { inner: bindings } = toml::from_str(slim_config).unwrap();

    let g = ModifiedKey::from(VirtualKeyCode::G);
    let cap_g = ModifiedKey(Key::from(VirtualKeyCode::G), ModifiersState::SHIFT);
    let j = ModifiedKey::from(VirtualKeyCode::J);

    let bindings = Keybindings::new(bindings);
    let mut key_combos = KeyCombos::new(bindings).unwrap();

    // Invalid combo 'gG' where the key that broke us out is a singlekey combo
    assert!(key_combos.munch(g).is_none());
    assert_eq!(Action::ToBottom, key_combos.munch(cap_g).unwrap());

    // Valid combo 'gj' that shares a branch with 'gg'
    assert!(key_combos.munch(g).is_none());
    assert_eq!(Action::ScrollDown, key_combos.munch(j).unwrap());

    // Valid singlekey combo for a shared action
    assert_eq!(Action::ScrollDown, key_combos.munch(j).unwrap());
}
