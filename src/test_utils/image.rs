use crate::image::ImageData;

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
    pub fn pre_decode(self) -> Vec<u8> {
        match self {
            Self::Jpg(jpg) => match jpg {
                SampleJpg::Rgb8 => include_bytes!("../../assets/test_data/rgb8.jpg").as_slice(),
                SampleJpg::Rgb8a => include_bytes!("../../assets/test_data/rgba8.jpg").as_slice(),
            },
            Self::Gif(gif) => match gif {
                SampleGif::AtuinDemo => {
                    include_bytes!("../../assets/test_data/atuin_demo.gif").as_slice()
                }
                SampleGif::Rgb8 => include_bytes!("../../assets/test_data/rgb8.gif").as_slice(),
                SampleGif::Rgba8 => include_bytes!("../../assets/test_data/rgba8.gif").as_slice(),
            },
            Self::Png(png) => match png {
                SamplePng::Ariadne => {
                    include_bytes!("../../assets/test_data/ariadne_example.png").as_slice()
                }
                SamplePng::Bun => include_bytes!("../../assets/test_data/bun_logo.png").as_slice(),
                SamplePng::Rgb8 => include_bytes!("../../assets/test_data/rgb8.png").as_slice(),
                SamplePng::Rgba8 => include_bytes!("../../assets/test_data/rgba8.png").as_slice(),
            },
            Self::Qoi(qoi) => match qoi {
                SampleQoi::Rgb8 => include_bytes!("../../assets/test_data/rgb8.qoi").as_slice(),
                SampleQoi::Rgba8 => include_bytes!("../../assets/test_data/rgba8.qoi").as_slice(),
            },
            Self::Svg(svg) => match svg {
                SampleSvg::Corro => include_bytes!("../../assets/test_data/corro.svg"),
                SampleSvg::Cargo => {
                    include_bytes!("../../assets/test_data/sample_cargo_badge.svg").as_slice()
                }
                SampleSvg::Repology => {
                    include_bytes!("../../assets/test_data/sample_repology_badge.svg").as_slice()
                }
            },
            Self::Webp(SampleWebp::CargoPublicApi) => {
                include_bytes!("../../assets/test_data/cargo_public_api.webp").as_slice()
            }
        }
        .into()
    }

    // TODO: adapt this to work with svg images too
    pub fn post_decode(self) -> ImageData {
        ImageData::load(&self.pre_decode(), true).unwrap()
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Sample::Gif(_) => "image/gif",
            Sample::Jpg(_) => "image/jpeg",
            Sample::Png(_) => "image/png",
            Sample::Qoi(_) => "image/qoi",
            Sample::Svg(_) => "image/svg+xml",
            Sample::Webp(_) => "image/webp",
        }
    }
}
