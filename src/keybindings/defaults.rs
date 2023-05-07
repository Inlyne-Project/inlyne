use super::{Action, Key, KeyCombo, Keybindings, ModifiedKey};

use winit::event::{ModifiersState, VirtualKeyCode};

const IS_MACOS: bool = cfg!(target_os = "macos");

pub fn defaults() -> Keybindings {
    let ctrl_or_command = if IS_MACOS {
        ModifiersState::LOGO
    } else {
        ModifiersState::CTRL
    };

    vec![
        // Copy: Ctrl+C / Command+C
        (
            Action::Copy,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtualKeyCode::C),
                ctrl_or_command,
            )]),
        ),
        // Zoom in: Ctrl++ / Command++
        (
            Action::ZoomIn,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtualKeyCode::Equals),
                ctrl_or_command | ModifiersState::SHIFT,
            )]),
        ),
        (
            Action::ZoomIn,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtualKeyCode::Plus),
                ctrl_or_command | ModifiersState::SHIFT,
            )]),
        ),
        // Zoom out: Ctrl+- / Command+-
        (
            Action::ZoomOut,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtualKeyCode::Minus),
                ctrl_or_command,
            )]),
        ),
        // Zoom reset: Ctrl+= / Command+=
        (
            Action::ZoomReset,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtualKeyCode::Equals),
                ctrl_or_command,
            )]),
        ),
        // Scroll up: Up-arrow
        (
            Action::ScrollUp,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::Up)]),
        ),
        // Scroll down: Down-arrow
        (
            Action::ScrollDown,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::Down)]),
        ),
        // Go to top of doc: Home
        (
            Action::ToTop,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::Home)]),
        ),
        // Go to bottom of doc: End
        (
            Action::ToBottom,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::End)]),
        ),
        // Quit: Esc
        (
            Action::Quit,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::Escape)]),
        ),
        // vim-like bindings
        // Copy: y
        (
            Action::Copy,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::Y)]),
        ),
        // Scroll up: k
        (
            Action::ScrollUp,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::K)]),
        ),
        // Scroll down: j
        (
            Action::ScrollDown,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::J)]),
        ),
        // Go to top of doc: gg
        (
            Action::ToTop,
            KeyCombo(vec![
                ModifiedKey::from(VirtualKeyCode::G),
                ModifiedKey::from(VirtualKeyCode::G),
            ]),
        ),
        // Go to bottom of doc: G
        (
            Action::ToBottom,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtualKeyCode::G),
                ModifiersState::SHIFT,
            )]),
        ),
        // Quit: q / ZZ / ZQ
        (
            Action::Quit,
            KeyCombo(vec![ModifiedKey::from(VirtualKeyCode::Q)]),
        ),
        (
            Action::Quit,
            KeyCombo(vec![
                ModifiedKey::from(VirtualKeyCode::Z),
                ModifiedKey::from(VirtualKeyCode::Z),
            ]),
        ),
        (
            Action::Quit,
            KeyCombo(vec![
                ModifiedKey::from(VirtualKeyCode::Z),
                ModifiedKey::from(VirtualKeyCode::Q),
            ]),
        ),
    ]
}
