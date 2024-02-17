#[cfg(feature = "wayland")]
use copypasta::{nop_clipboard::NopClipboardContext as ClipboardContext, ClipboardProvider};
#[cfg(feature = "x11")]
use copypasta::{ClipboardContext, ClipboardProvider};

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
}

impl Clipboard {
    pub fn new() -> Self {
        let clipboard = Box::new(ClipboardContext::new().unwrap());
        Self { clipboard }
    }

    pub fn set_contents(&mut self, text: impl Into<String>) {
        self.clipboard.set_contents(text.into()).unwrap();
    }
}
