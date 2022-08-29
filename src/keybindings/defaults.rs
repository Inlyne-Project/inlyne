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
    ]
}
