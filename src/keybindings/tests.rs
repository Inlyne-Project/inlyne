use super::{
    action::{Action, VertDirection},
    Key, KeyCombos, ModifiedKey,
};
use crate::{keybindings::Keybindings, opts::Config, test_utils::init_test_log};

use winit::event::{ModifiersState, VirtualKeyCode};

#[test]
fn sanity() {
    init_test_log();

    let config = r#"
[keybindings]
base = [
    ["ToTop", ["g", "g"]],
    ["ToBottom", { key = "g", mod = ["Shift"] }],
    ["ScrollDown", ["g", "j"]],
    ["ScrollDown", "j"],
]
"#;

    let Config { keybindings, .. } = Config::load_from_str(config).unwrap();
    let mut bindings = keybindings.base.unwrap_or_else(Keybindings::empty);
    bindings.extend(keybindings.extra.unwrap_or_else(Keybindings::empty));
    let mut key_combos = KeyCombos::new(bindings).unwrap();

    let g = ModifiedKey::from(VirtualKeyCode::G);
    let cap_g = ModifiedKey(Key::from(VirtualKeyCode::G), ModifiersState::SHIFT);
    let j = ModifiedKey::from(VirtualKeyCode::J);

    // Invalid combo 'gG' where the key that broke us out is a singlekey combo
    assert!(key_combos.munch(g).is_none());
    assert_eq!(
        Action::ToEdge(VertDirection::Down),
        key_combos.munch(cap_g).unwrap()
    );

    // Valid combo 'gj' that shares a branch with 'gg'
    assert!(key_combos.munch(g).is_none());
    assert_eq!(
        Action::Scroll(VertDirection::Down),
        key_combos.munch(j).unwrap()
    );

    // Valid singlekey combo for a shared action
    assert_eq!(
        Action::Scroll(VertDirection::Down),
        key_combos.munch(j).unwrap()
    );
}
