use winit::window::CursorIcon;

#[derive(Debug, Clone)]
pub struct Rect {
    pub pos: (f32, f32),
    pub size: (f32, f32),
    // Cache max point on creation
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

    pub fn contains(&self, loc: (f32, f32)) -> bool {
        self.pos.0 <= loc.0 && loc.0 <= self.max.0 && self.pos.1 <= loc.1 && loc.1 <= self.max.1
    }
}

impl From<wgpu_glyph::ab_glyph::Rect> for Rect {
    fn from(other_rect: wgpu_glyph::ab_glyph::Rect) -> Self {
        Self {
            pos: (other_rect.min.x, other_rect.min.y),
            size: (
                other_rect.max.x - other_rect.min.x,
                other_rect.max.y - other_rect.min.y,
            ),
            max: (other_rect.max.x, other_rect.max.y),
        }
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

impl From<CursorIcon> for HoverInfo {
    fn from(cursor_icon: CursorIcon) -> Self {
        Self {
            cursor_icon,
            ..Default::default()
        }
    }
}
