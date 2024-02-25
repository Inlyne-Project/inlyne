#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Navigate(Navigation),
    ToEdge(VertDirection),
    Scroll(VertDirection),
    Page(VertDirection),
    Zoom(Zoom),
    Copy,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zoom {
    In,
    Out,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    Previous,
    Next,
}
