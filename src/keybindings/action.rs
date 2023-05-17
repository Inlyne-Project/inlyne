use serde::Deserialize;

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ToTop,
    ToBottom,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Copy,
    Quit,
}
