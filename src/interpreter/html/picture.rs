use crate::image::ImageSize;
use crate::opts::ResolvedTheme;
use crate::utils::Align;

use anyhow::Context;

#[derive(Debug, Default)]
pub struct Inner {
    pub align: Option<Align>,
    pub dark_variant: Option<String>,
    pub light_variant: Option<String>,
    pub size: Option<ImageSize>,
}

#[derive(Debug, Default)]
pub struct Builder {
    inner: Inner,
    src: Option<String>,
}

impl Builder {
    pub fn set_align(&mut self, align: Align) {
        self.inner.align = Some(align);
    }

    pub fn set_dark_variant(&mut self, dark: String) {
        self.inner.dark_variant = Some(dark);
    }

    pub fn set_light_variant(&mut self, light: String) {
        self.inner.light_variant = Some(light);
    }

    pub fn set_size(&mut self, size: ImageSize) {
        self.inner.size = Some(size);
    }

    pub fn set_src(&mut self, src: String) {
        self.src = Some(src);
    }

    pub fn try_finish(self) -> anyhow::Result<Picture> {
        let Self { inner, src } = self;
        let src = src.context("Missing `src` link for <picture>")?;
        Ok(Picture { inner, src })
    }
}

#[derive(Debug)]
pub struct Picture {
    pub inner: Inner,
    pub src: String,
}

impl Picture {
    pub fn builder() -> Builder {
        Builder::default()
    }

    pub fn resolve_src(&self, scheme: Option<ResolvedTheme>) -> &str {
        scheme
            .and_then(|scheme| match scheme {
                ResolvedTheme::Dark => self.inner.dark_variant.as_ref(),
                ResolvedTheme::Light => self.inner.light_variant.as_ref(),
            })
            .unwrap_or(&self.src)
    }
}
