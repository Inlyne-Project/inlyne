use wgpu_glyph::ab_glyph;
use winit::window::CursorIcon;

#[derive(Debug, Clone)]
pub struct Rect {
    pub pos: (f32, f32),
    pub size: (f32, f32),
    pub max: (f32, f32),
}

impl Rect {
    pub fn new(pos: (f32, f32), size: (f32, f32)) -> Rect {
        Rect {
            pos,
            size,
            max: (pos.0 + size.0, pos.1 + size.1),
        }
    }

    pub fn from_min_max(min: (f32, f32), max: (f32, f32)) -> Rect {
        Rect {
            pos: min,
            size: (max.0 - min.0, max.1 - min.1),
            max,
        }
    }

    pub fn contains(&self, loc: (f32, f32)) -> bool {
        self.pos.0 <= loc.0 && loc.0 <= self.max.0 && self.pos.1 <= loc.1 && loc.1 <= self.max.1
    }
}

impl From<ab_glyph::Rect> for Rect {
    fn from(other_rect: ab_glyph::Rect) -> Self {
        let ab_glyph::Rect {
            min: ab_glyph::Point { x: min_x, y: min_y },
            max: ab_glyph::Point { x: max_x, y: max_y },
        } = other_rect;
        Self::from_min_max((min_x, min_y), (max_x, max_y))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Default)]
pub struct HoverInfo {
    pub cursor_icon: CursorIcon,
    pub jump: Option<f32>,
}
