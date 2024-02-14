use super::action::{Action, VertDirection};
use super::{KeyCombos, ModifiedKey};
use crate::opts::Config;
use crate::test_utils::init_test_log;

use winit::event::{ModifiersState, VirtualKeyCode as VirtKey};

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

    // TODO: move this to a helper somewhere
    let Config { keybindings, .. } = Config::load_from_str(config).unwrap();
    let mut key_combos = KeyCombos::new(keybindings).unwrap();

    let g: ModifiedKey = VirtKey::G.into();
    let l_shift = VirtKey::LShift.into();
    let cap_g = ModifiedKey(g.0, ModifiersState::SHIFT);
    let j = VirtKey::J.into();

    let test_vectors = [
        // Invalid combo 'gG' where the key that broke us out is a singlekey combo
        (g, None),
        (l_shift, None),
        (cap_g, Some(Action::ToEdge(VertDirection::Down))),
        // Valid combo 'gg' that shares a branch with 'gj'
        (g, None),
        (g, Some(Action::ToEdge(VertDirection::Up))),
        // Valid singlekey combo for a shared action
        (j, Some(Action::Scroll(VertDirection::Down))),
    ];

    for (key, maybe_action) in test_vectors {
        assert_eq!(key_combos.munch(key), maybe_action);
    }
}
