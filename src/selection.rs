use crate::utils::{dist_between_points, Point};
use std::time::{Duration, Instant};

const CLICK_TOLERANCE: Duration = Duration::from_millis(300);
const MAX_CLICK_DIST: f32 = 5.0;

#[derive(PartialEq, Debug)]
pub enum SelectionMode {
    Word,
    Line,
}

#[derive(Debug, Default)]
pub enum SelectionKind {
    Drag {
        start: Point,
        end: Point,
    },
    Click {
        mode: SelectionMode,
        time: Instant,
        position: Point,
    },
    Start {
        position: Point,
        time: Instant,
    },
    #[default]
    None,
}

#[derive(Default)]
pub struct Selection {
    pub selection: SelectionKind,
    pub text: String,
}

impl Selection {
    pub const fn new() -> Self {
        Self {
            selection: SelectionKind::None,
            text: String::new(),
        }
    }
    pub fn is_none(&self) -> bool {
        matches!(self.selection, SelectionKind::None)
    }
    pub fn start(&mut self, position: Point) {
        self.selection = SelectionKind::Start {
            position,
            time: Instant::now(),
        }
    }

    #[must_use]
    pub fn handle_drag(&mut self, new_position: Point) -> bool {
        self.text.clear();
        match &mut self.selection {
            SelectionKind::Start { position, .. } => {
                self.selection = SelectionKind::Drag {
                    start: *position,
                    end: new_position,
                };
            }
            SelectionKind::Drag { end, .. } => *end = new_position,
            _ => return false,
        }
        true
    }

    pub fn add_position(&mut self, new_position: Point) {
        self.text.clear();

        match &self.selection {
            SelectionKind::Click {
                mode,
                time,
                position,
            } => {
                if mode == &SelectionMode::Word
                    && time.elapsed() < CLICK_TOLERANCE
                    && dist_between_points(position, &new_position) < MAX_CLICK_DIST
                {
                    self.selection = SelectionKind::Click {
                        time: Instant::now(),
                        mode: SelectionMode::Line,
                        position: new_position,
                    };
                } else {
                    self.start(new_position)
                }
            }
            SelectionKind::Start { position, time } => {
                if time.elapsed() < CLICK_TOLERANCE
                    && dist_between_points(position, &new_position) < MAX_CLICK_DIST
                {
                    self.selection = SelectionKind::Click {
                        time: Instant::now(),
                        mode: SelectionMode::Word,
                        position: new_position,
                    };
                } else {
                    self.start(new_position)
                }
            }
            _ => self.start(new_position),
        }
    }

    pub fn add_line(&mut self, str: &str) {
        self.text.push_str(str);
        self.text.push('\n');
    }
}
