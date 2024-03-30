use crate::image::{
    cache::{StableImage, SvgContext},
    ImageData,
};

#[derive(Clone, Copy)]
pub enum Sample {
    Gif(SampleGif),
    Jpg(SampleJpg),
    Png(SamplePng),
    Qoi(SampleQoi),
    Svg(SampleSvg),
    Webp(SampleWebp),
}

impl From<SampleGif> for Sample {
    fn from(gif: SampleGif) -> Self {
        Self::Gif(gif)
    }
}

impl From<SampleJpg> for Sample {
    fn from(jpg: SampleJpg) -> Self {
        Self::Jpg(jpg)
    }
}

impl From<SamplePng> for Sample {
    fn from(png: SamplePng) -> Self {
        Self::Png(png)
    }
}

impl From<SampleQoi> for Sample {
    fn from(qoi: SampleQoi) -> Self {
        Self::Qoi(qoi)
    }
}

impl From<SampleSvg> for Sample {
    fn from(svg: SampleSvg) -> Self {
        Self::Svg(svg)
    }
}

impl From<SampleWebp> for Sample {
    fn from(webp: SampleWebp) -> Self {
        Self::Webp(webp)
    }
}

#[derive(Clone, Copy)]
pub enum SampleGif {
    AtuinDemo,
    Rgb8,
    Rgba8,
}

#[derive(Clone, Copy)]
pub enum SampleJpg {
    Rgb8,
    Rgb8a,
}

#[derive(Clone, Copy)]
pub enum SamplePng {
    Ariadne,
    Bun,
    Rgb8,
    Rgba8,
}

#[derive(Clone, Copy)]
pub enum SampleQoi {
    Rgb8,
    Rgba8,
}

#[derive(Clone, Copy)]
pub enum SampleSvg {
    Corro,
    Cargo,
    Repology,
}

#[derive(Clone, Copy)]
pub enum SampleWebp {
    CargoPublicApi,
}

impl Sample {
    pub fn pre_decode(self) -> &'static [u8] {
        match self {
            Self::Jpg(jpg) => match jpg {
                SampleJpg::Rgb8 => include_bytes!("../../assets/test_data/rgb8.jpg"),
                SampleJpg::Rgb8a => include_bytes!("../../assets/test_data/rgba8.jpg"),
            },
            Self::Gif(gif) => match gif {
                SampleGif::AtuinDemo => {
                    include_bytes!("../../assets/test_data/atuin_demo.gif")
                }
                SampleGif::Rgb8 => include_bytes!("../../assets/test_data/rgb8.gif"),
                SampleGif::Rgba8 => include_bytes!("../../assets/test_data/rgba8.gif"),
            },
            Self::Png(png) => match png {
                SamplePng::Ariadne => {
                    include_bytes!("../../assets/test_data/ariadne_example.png")
                }
                SamplePng::Bun => include_bytes!("../../assets/test_data/bun_logo.png"),
                SamplePng::Rgb8 => include_bytes!("../../assets/test_data/rgb8.png"),
                SamplePng::Rgba8 => include_bytes!("../../assets/test_data/rgba8.png"),
            },
            Self::Qoi(qoi) => match qoi {
                SampleQoi::Rgb8 => include_bytes!("../../assets/test_data/rgb8.qoi"),
                SampleQoi::Rgba8 => include_bytes!("../../assets/test_data/rgba8.qoi"),
            },
            Self::Svg(svg) => match svg {
                SampleSvg::Corro => include_bytes!("../../assets/test_data/corro.svg"),
                SampleSvg::Cargo => {
                    include_bytes!("../../assets/test_data/sample_cargo_badge.svg")
                }
                SampleSvg::Repology => {
                    include_bytes!("../../assets/test_data/sample_repology_badge.svg")
                }
            },
            Self::Webp(SampleWebp::CargoPublicApi) => {
                include_bytes!("../../assets/test_data/cargo_public_api.webp")
            }
        }
    }

    // TODO: replace this with the common image loading function
    pub fn post_decode(self, svg_ctx: &SvgContext) -> ImageData {
        if let Self::Svg(_) = self {
            let text = std::str::from_utf8(self.pre_decode()).unwrap();
            let image = StableImage::from_svg(text);
            image.render(svg_ctx).unwrap()
        } else {
            ImageData::load(&self.pre_decode(), true).unwrap()
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::Gif(_) => ".gif",
            Self::Jpg(_) => ".jpg",
            Self::Png(_) => ".png",
            Self::Qoi(_) => ".qoi",
            Self::Svg(_) => ".svg",
            Self::Webp(_) => ".webp",
        }
    }
}
