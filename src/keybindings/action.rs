#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    History(HistDirection),
    ToEdge(VertDirection),
    Scroll(VertDirection),
    Page(VertDirection),
    Zoom(Zoom),
    Copy,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistDirection {
    Next,
    Prev,
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
