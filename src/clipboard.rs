#![allow(unused)]

#[cfg(any(test, not(any(feature = "x11", target_os = "macos", windows))))]
use copypasta::nop_clipboard::NopClipboardContext;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use copypasta::wayland_clipboard;
#[cfg(any(feature = "x11", target_os = "macos", windows))]
use copypasta::ClipboardContext;

use copypasta::ClipboardProvider;
use raw_window_handle::RawDisplayHandle;

pub struct Clipboard(Box<dyn ClipboardProvider>);

impl Clipboard {
    pub unsafe fn new(display: RawDisplayHandle) -> Self {
        match display {
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            RawDisplayHandle::Wayland(display) => {
                let (_, clipboard) =
                    wayland_clipboard::create_clipboards_from_external(display.display);
                Self(Box::new(clipboard))
            }
            _ => Self::default(),
        }
    }

    /// Used for tests and to handle missing clipboard provider when built without the `x11`
    /// feature.
    #[cfg(any(test, not(any(feature = "x11", target_os = "macos", windows))))]
    pub fn new_nop() -> Self {
        let clipboard = Box::new(NopClipboardContext::new().unwrap());
        Self(clipboard)
    }

    pub fn set_contents(&mut self, text: impl Into<String>) {
        self.0.set_contents(text.into()).unwrap_or_else(|err| {
            tracing::warn!("Unable to store text in clipboard: {}", err);
        });
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        #[cfg(any(target_os = "macos", windows))]
        return Self(Box::new(ClipboardContext::new().unwrap()));

        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        return Self(Box::new(ClipboardContext::new().unwrap()));

        #[cfg(not(any(feature = "x11", target_os = "macos", windows)))]
        return Self::new_nop();
    }
}
