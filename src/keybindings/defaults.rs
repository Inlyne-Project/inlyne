use super::{Action, Key, KeyCombo, Keybindings, ModifiedKey};

use winit::event::{ModifiersState, VirtualKeyCode as VirtKey};

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
            KeyCombo(vec![ModifiedKey(Key::from(VirtKey::C), ctrl_or_command)]),
        ),
        // Zoom in: Ctrl++ / Command++
        (
            Action::ZoomIn,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::Equals),
                ctrl_or_command | ModifiersState::SHIFT,
            )]),
        ),
        (
            Action::ZoomIn,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::Plus),
                ctrl_or_command | ModifiersState::SHIFT,
            )]),
        ),
        // Zoom out: Ctrl+- / Command+-
        (
            Action::ZoomOut,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::Minus),
                ctrl_or_command,
            )]),
        ),
        // Zoom reset: Ctrl+= / Command+=
        (
            Action::ZoomReset,
            KeyCombo(vec![ModifiedKey(
                Key::from(VirtKey::Equals),
                ctrl_or_command,
            )]),
        ),
        // Scroll up: Up-arrow
        (Action::ScrollUp, KeyCombo::from(VirtKey::Up)),
        // Scroll down: Down-arrow
        (Action::ScrollDown, KeyCombo::from(VirtKey::Down)),
        // Go to top of doc: Home
        (Action::ToTop, KeyCombo::from(VirtKey::Home)),
        // Go to bottom of doc: End
        (Action::ToBottom, KeyCombo::from(VirtKey::End)),
        // Quit: Esc
        (Action::Quit, KeyCombo::from(VirtKey::Escape)),
        // vim-like bindings
        // Copy: y
        (Action::Copy, KeyCombo::from(VirtKey::Y)),
        // Scroll up: k
        (Action::ScrollUp, KeyCombo::from(VirtKey::K)),
        // Scroll down: j
        (Action::ScrollDown, KeyCombo::from(VirtKey::J)),
        // Go to top of doc: gg
        (
            Action::ToTop,
            KeyCombo(vec![
                ModifiedKey::from(VirtKey::G),
                ModifiedKey::from(VirtKey::G),
            ]),
        ),
        // Go to bottom of doc: G
        (
            Action::ToBottom,
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
                ModifiedKey::from(VirtKey::Z),
                ModifiedKey::from(VirtKey::Z),
            ]),
        ),
        (
            Action::Quit,
            KeyCombo(vec![
                ModifiedKey::from(VirtKey::Z),
                ModifiedKey::from(VirtKey::Q),
            ]),
        ),
    ]
}
