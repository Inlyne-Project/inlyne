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

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub text_color: [f32; 4],
    pub clear_color: wgpu::Color,
    pub code_color: [f32; 4],
    pub code_block_color: [f32; 4],
    pub link_color: [f32; 4],
    pub select_color: [f32; 4],
    pub code_highlighter: &'static str,
}

pub const DARK_DEFAULT: Theme = Theme {
    text_color: [0.5841, 0.6376, 0.6939, 1.0],
    clear_color: wgpu::Color {
        r: 0.0040,
        g: 0.0056,
        b: 0.0086,
        a: 1.0,
    },
    code_color: [1. * 0.7, 0.0578 * 0.7, 0.9331 * 0.7, 1.0],
    code_block_color: [0.0080 * 1.5, 0.0110 * 1.5, 0.0156 * 1.5, 1.0],
    link_color: [0.0976, 0.3813, 1.0, 1.0],
    select_color: [0.17, 0.22, 0.3, 1.0],
    code_highlighter: "base16-mocha.dark",
};

pub const LIGHT_DEFAULT: Theme = Theme {
    text_color: [0., 0., 0., 1.0],
    clear_color: wgpu::Color::WHITE,
    code_color: [1., 0.0578, 0.9331, 1.0],
    code_block_color: [0.92, 0.92, 0.92, 1.0],
    link_color: [0.0975, 0.1813, 1.0, 1.0],
    select_color: [0.67, 0.85, 0.9, 1.0],
    code_highlighter: "InspiredGitHub",
};
