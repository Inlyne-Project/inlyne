use rand::seq::IteratorRandom;
use rand::thread_rng;

pub fn default_palette() -> [[f32; 4]; 7] {
    [
        hex_to_linear_rgba(0xA8006D),
        hex_to_linear_rgba(0xE43F47),
        hex_to_linear_rgba(0xFF822F),
        hex_to_linear_rgba(0xF8CE18),
        hex_to_linear_rgba(0x6BAA2C),
        hex_to_linear_rgba(0x1E9FD2),
        hex_to_linear_rgba(0x6B46C1),
    ]
}

pub fn hex_to_linear_rgba(c: u32) -> [f32; 4] {
    let f = |xu: u32| {
        let x = (xu & 0xFF) as f32 / 255.0;
        if x > 0.04045 {
            ((x + 0.055) / 1.055).powf(2.4)
        } else {
            x / 12.92
        }
    };
    [f(c >> 16), f(c >> 8), f(c), 1.0]
}

pub struct ColorPool {
    pool: Vec<([f32; 4], bool)>,
}

impl ColorPool {
    pub fn new(colors: &[[f32; 4]]) -> Self {
        let pool = colors.iter().map(|c| (*c, true)).collect();
        Self { pool }
    }
    pub fn random_color(&mut self) -> Option<[f32; 4]> {
        let mut rng = thread_rng();
        self.pool
            .iter_mut()
            .filter(|c| c.1)
            .choose(&mut rng)
            .map(|c| {
                c.1 = false;
                c.0
            })
    }
}
