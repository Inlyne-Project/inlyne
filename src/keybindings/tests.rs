use super::action::{Action, VertDirection};
use super::{KeyCombos, Keybindings, ModifiedKey};
use crate::opts::Config;
use crate::test_utils::log;

use winit::event::{ModifiersState, VirtualKeyCode as VirtKey};

#[test]
fn sanity() {
    log::init();

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

// TODO(cosmic): Move this to reading from the `inlyne.default.toml` file after a bit of cleanup to
// make things less verbose
// TODO(cosmic): Consider switching the casing away from PascalCase? Maybe keep it inline with the
// rest of the config file and use kebab-case instead?
const DEFAULTS_TEMPLATE: &str = r#"
[keybindings]
base = [
    # Regular
    ["Copy", { key = "c", mod = "CTRL_OR_CMD" }],
    ["ZoomIn", { key = "=", mod = "CTRL_OR_CMD" }],
    ["ZoomOut", { key = "-", mod = "CTRL_OR_CMD" }],
    ["HistoryNext", { key = "Right", mod = "Alt" }],
    ["HistoryPrevious", { key = "Left", mod = "Alt" }],
    ["ScrollUp", "Up"],
    ["ScrollDown", "Down"],
    ["PageUp", "PageUp"],
    ["PageDown", "PageDown"],
    ["ToTop", "Home"],
    ["ToBottom", "End"],
    ["Quit", "Escape"],
    # Vim-like
    ["Copy", "y"],
    ["ScrollUp", "k"],
    ["ScrollDown", "j"],
    ["ToTop", ["g", "g"]],
    ["ToBottom", "G"],
    ["Quit", "q"],
    ["Quit", ["Z", "Z"]],
    ["Quit", ["Z", "Q"]],
    ["HistoryNext", ["b", "n"]],
    ["HistoryPrevious", ["b", "p"]],
]
"#;

#[test]
fn link_hints_is_opt_in() {
    assert!(!Keybindings::default()
        .iter()
        .any(|(action, _)| action == &Action::LinkHints));

    for (config, keys, expected) in [
        ("", vec![VirtKey::F], None),
        ("[keybindings]", vec![VirtKey::F], None),
        (
            include_str!("../../assets/manual_test_data/link_hints.toml"),
            vec![VirtKey::F],
            Some(Action::LinkHints),
        ),
        (
            r#"[keybindings]
extra = [["Copy", "f"]]"#,
            vec![VirtKey::F],
            Some(Action::Copy),
        ),
        // Leaving hints disabled preserves existing user combos beginning with f.
        (
            r#"[keybindings]
extra = [["Copy", ["f", "g"]]]"#,
            vec![VirtKey::F, VirtKey::G],
            Some(Action::Copy),
        ),
        (
            r#"[keybindings]
base = [["LinkHints", "l"]]"#,
            vec![VirtKey::L],
            Some(Action::LinkHints),
        ),
        (
            r#"[keybindings]
base = [["LinkHints", "l"]]"#,
            vec![VirtKey::F],
            None,
        ),
    ] {
        let Config { keybindings, .. } = Config::load_from_str(config).unwrap();
        let mut combos = KeyCombos::new(keybindings).unwrap();
        let last = keys
            .into_iter()
            .map(|key| combos.munch(key.into()))
            .last()
            .unwrap();
        assert_eq!(last, expected);
    }
}

#[test]
fn opting_into_link_hints_preserves_other_defaults() {
    let Config { keybindings, .. } = Config::load_from_str(include_str!(
        "../../assets/manual_test_data/link_hints.toml"
    ))
    .unwrap();
    let mut combos = KeyCombos::new(keybindings).unwrap();
    assert_eq!(combos.munch(VirtKey::F.into()), Some(Action::LinkHints));
    assert_eq!(
        combos.munch(VirtKey::J.into()),
        Some(Action::Scroll(VertDirection::Down))
    );
    assert_eq!(combos.munch(VirtKey::Escape.into()), Some(Action::Quit));
}

#[test]
fn link_hints_supports_an_opt_in_key_sequence() {
    let Config { keybindings, .. } = Config::load_from_str(
        r#"[keybindings]
extra = [["LinkHints", ["f", "g"]]]"#,
    )
    .unwrap();
    let mut combos = KeyCombos::new(keybindings).unwrap();
    assert_eq!(combos.munch(VirtKey::F.into()), None);
    assert_eq!(combos.munch(VirtKey::G.into()), Some(Action::LinkHints));
}

#[test]
fn defaults() {
    let ctrl_or_cmd = if cfg!(target_os = "macos") {
        "Os"
    } else {
        "Ctrl"
    };
    let defaults = DEFAULTS_TEMPLATE.replace("CTRL_OR_CMD", ctrl_or_cmd);
    let Config { keybindings, .. } = Config::load_from_str(&defaults).unwrap();
    let config_defaults: Keybindings = keybindings.into();

    let internal_defaults = Keybindings(super::defaults::defaults());
    assert_eq!(config_defaults, internal_defaults);
}
