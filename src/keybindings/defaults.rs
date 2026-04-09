use crate::keybindings::action::HistDirection;

use super::action::{Action, VertDirection, Zoom};
use super::{Key, KeyCombo, ModifiedKey};

use winit::keyboard::{ModifiersState, NamedKey};

const IS_MACOS: bool = cfg!(target_os = "macos");

pub fn defaults() -> Vec<(Action, KeyCombo)> {
    let ctrl_or_command = if IS_MACOS {
        ModifiersState::SUPER
    } else {
        ModifiersState::CONTROL
    };

    vec![
        // Copy: Ctrl+C / Command+C
        (
            Action::Copy,
            KeyCombo(vec![ModifiedKey(Key::Character("c".into()), ctrl_or_command)]),
        ),
        // Zoom in: Ctrl+= / Command+=
        (
            Action::Zoom(Zoom::In),
            KeyCombo(vec![ModifiedKey(
                Key::Character("=".into()),
                ctrl_or_command,
            )]),
        ),
        // Zoom out: Ctrl+- / Command+-
        (
            Action::Zoom(Zoom::Out),
            KeyCombo(vec![ModifiedKey(
                Key::Character("-".into()),
                ctrl_or_command,
            )]),
        ),
        // Navigate to next file: Alt+Right
        (
            Action::History(HistDirection::Next),
            KeyCombo(vec![ModifiedKey(
                Key::Named(NamedKey::ArrowRight),
                ModifiersState::ALT,
            )]),
        ),
        // Navigate to previous file: Alt+Left
        (
            Action::History(HistDirection::Prev),
            KeyCombo(vec![ModifiedKey(
                Key::Named(NamedKey::ArrowLeft),
                ModifiersState::ALT,
            )]),
        ),
        // Scroll up: Up-arrow
        (
            Action::Scroll(VertDirection::Up),
            KeyCombo::from(NamedKey::ArrowUp),
        ),
        // Scroll down: Down-arrow
        (
            Action::Scroll(VertDirection::Down),
            KeyCombo::from(NamedKey::ArrowDown),
        ),
        // Page up: PageUp
        (
            Action::Page(VertDirection::Up),
            KeyCombo::from(NamedKey::PageUp),
        ),
        // Page down: PageDown
        (
            Action::Page(VertDirection::Down),
            KeyCombo::from(NamedKey::PageDown),
        ),
        // Go to top of doc: Home
        (
            Action::ToEdge(VertDirection::Up),
            KeyCombo::from(NamedKey::Home),
        ),
        // Go to bottom of doc: End
        (
            Action::ToEdge(VertDirection::Down),
            KeyCombo::from(NamedKey::End),
        ),
        // Quit: Esc
        (Action::Quit, KeyCombo::from(NamedKey::Escape)),
        // vim-like bindings
        // Copy: y
        (Action::Copy, KeyCombo::from(Key::Character("y".into()))),
        // Scroll up: k
        (
            Action::Scroll(VertDirection::Up),
            KeyCombo::from(Key::Character("k".into())),
        ),
        // Scroll down: j
        (
            Action::Scroll(VertDirection::Down),
            KeyCombo::from(Key::Character("j".into())),
        ),
        // Go to top of doc: gg
        (
            Action::ToEdge(VertDirection::Up),
            KeyCombo(vec![
                ModifiedKey::from(Key::Character("g".into())),
                ModifiedKey::from(Key::Character("g".into())),
            ]),
        ),
        // Go to bottom of doc: G
        (
            Action::ToEdge(VertDirection::Down),
            KeyCombo(vec![ModifiedKey(
                Key::Character("g".into()),
                ModifiersState::SHIFT,
            )]),
        ),
        // Quit: q / ZZ / ZQ
        (Action::Quit, KeyCombo::from(Key::Character("q".into()))),
        (
            Action::Quit,
            KeyCombo(vec![
                ModifiedKey(Key::Character("z".into()), ModifiersState::SHIFT),
                ModifiedKey(Key::Character("z".into()), ModifiersState::SHIFT),
            ]),
        ),
        (
            Action::Quit,
            KeyCombo(vec![
                ModifiedKey(Key::Character("z".into()), ModifiersState::SHIFT),
                ModifiedKey(Key::Character("q".into()), ModifiersState::SHIFT),
            ]),
        ),
        // Navigate to next file: bn
        (
            Action::History(HistDirection::Next),
            KeyCombo(vec![
                ModifiedKey::from(Key::Character("b".into())),
                ModifiedKey::from(Key::Character("n".into())),
            ]),
        ),
        // Navigate to previous file: bp
        (
            Action::History(HistDirection::Prev),
            KeyCombo(vec![
                ModifiedKey::from(Key::Character("b".into())),
                ModifiedKey::from(Key::Character("p".into())),
            ]),
        ),
    ]
}
