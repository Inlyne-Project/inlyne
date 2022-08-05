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

pub struct Theme {
    pub text_color: [f32; 4],
    pub clear_color: wgpu::Color,
    pub code_color: [f32; 4],
    pub code_block_color: [f32; 4],
    pub link_color: [f32; 4],
}

pub const DARK_DEFAULT: Theme = Theme {
    text_color: [0.5840785, 0.63759696, 0.6938719, 1.0],
    clear_color: wgpu::Color {
        r: 0.004024717,
        g: 0.0056053917,
        b: 0.008568125,
        a: 1.0,
    },
    code_color: [0.5840785, 0.63759696, 0.6938719, 1.0],
    code_block_color: [0.008023192 * 1.5, 0.01096009 * 1.5, 0.015996292 * 1.5, 1.0],
    link_color: [0.09758736, 0.3813261, 1.0, 1.0],
};

pub const LIGHT_DEFAULT: Theme = Theme {
    text_color: [0., 0., 0., 1.0],
    clear_color: wgpu::Color::WHITE,
    code_color: [1., 0.057805434, 0.933104762, 1.0],
    code_block_color: [0.8, 0.8, 0.8, 1.0],
    link_color: [0.09758736, 0.1813261, 1.0, 1.0],
};
