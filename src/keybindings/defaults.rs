use crate::keybindings::action::HistDirection;

use super::action::{Action, VertDirection, Zoom};
use super::{Key, KeyCombo, ModifiedKey};

use winit::event::{ModifiersState, VirtualKeyCode as VirtKey};

const IS_MACOS: bool = cfg!(target_os = "macos");

pub fn defaults() -> Vec<(Action, KeyCombo)> {
    let ctrl_or_command = if IS_MACOS {
        ModifiersState::LOGO
    } else {
        ModifiersState::CTRL
    };

    vec![
        // Copy: Ctrl+C / Command+C
        (
            Action::Copy,
            KeyCombo(vec![ModifiedKey(Key::from(VirtKey::C), ctrl_or_command)]),
        ),
        // Zoom in: Ctrl+= / Command+=
        (
            Action::Zoom(Zoom::In),
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::Equals),
                ctrl_or_command,
            )]),
        ),
        // Zoom out: Ctrl+- / Command+-
        (
            Action::Zoom(Zoom::Out),
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::Minus),
                ctrl_or_command,
            )]),
        ),
        // Navigate to next file: Alt+Right
        (
            Action::History(HistDirection::Next),
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::Right),
                ModifiersState::ALT,
            )]),
        ),
        // Navigate to previous file: Alt+Left
        (
            Action::History(HistDirection::Prev),
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::Left),
                ModifiersState::ALT,
            )]),
        ),
        // Scroll up: Up-arrow
        (
            Action::Scroll(VertDirection::Up),
            KeyCombo::from(VirtKey::Up),
        ),
        // Scroll down: Down-arrow
        (
            Action::Scroll(VertDirection::Down),
            KeyCombo::from(VirtKey::Down),
        ),
        // Page up: PageUp
        (
            Action::Page(VertDirection::Up),
            KeyCombo::from(VirtKey::PageUp),
        ),
        // Page down: PageDown
        (
            Action::Page(VertDirection::Down),
            KeyCombo::from(VirtKey::PageDown),
        ),
        // Go to top of doc: Home
        (
            Action::ToEdge(VertDirection::Up),
            KeyCombo::from(VirtKey::Home),
        ),
        // Go to bottom of doc: End
        (
            Action::ToEdge(VertDirection::Down),
            KeyCombo::from(VirtKey::End),
        ),
        // Quit: Esc
        (Action::Quit, KeyCombo::from(VirtKey::Escape)),
        // vim-like bindings
        // Copy: y
        (Action::Copy, KeyCombo::from(VirtKey::Y)),
        // Scroll up: k
        (
            Action::Scroll(VertDirection::Up),
            KeyCombo::from(VirtKey::K),
        ),
        // Scroll down: j
        (
            Action::Scroll(VertDirection::Down),
            KeyCombo::from(VirtKey::J),
        ),
        // Go to top of doc: gg
        (
            Action::ToEdge(VertDirection::Up),
            KeyCombo(vec![
                ModifiedKey::from(VirtKey::G),
                ModifiedKey::from(VirtKey::G),
            ]),
        ),
        // Go to bottom of doc: G
        (
            Action::ToEdge(VertDirection::Down),
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::G),
                ModifiersState::SHIFT,
            )]),
        ),
        // Quit: q / ZZ / ZQ
        (Action::Quit, KeyCombo::from(VirtKey::Q)),
        (
            Action::Quit,
            KeyCombo(vec![
                ModifiedKey(Key::from(VirtKey::Z), ModifiersState::SHIFT),
                ModifiedKey(Key::from(VirtKey::Z), ModifiersState::SHIFT),
            ]),
        ),
        (
            Action::Quit,
            KeyCombo(vec![
                ModifiedKey(Key::from(VirtKey::Z), ModifiersState::SHIFT),
                ModifiedKey(Key::from(VirtKey::Q), ModifiersState::SHIFT),
            ]),
        ),
        // Navigate to next file: bn
        (
            Action::History(HistDirection::Next),
            KeyCombo(vec![
                ModifiedKey::from(VirtKey::B),
                ModifiedKey::from(VirtKey::N),
            ]),
        ),
        // Navigate to previous file: bp
        (
            Action::History(HistDirection::Prev),
            KeyCombo(vec![
                ModifiedKey::from(VirtKey::B),
                ModifiedKey::from(VirtKey::P),
            ]),
        ),
    ]
}
