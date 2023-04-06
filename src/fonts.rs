use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::SystemSource;
use serde::{Deserialize, Serialize};
use wgpu_glyph::ab_glyph::{FontArc, FontRef, FontVec};

use crate::opts::FontOptions;

#[derive(Deserialize, Serialize)]
struct FontCache {
    name: String,
    base: HandleCache,
    italic: HandleCache,
    bold: HandleCache,
    bold_italic: HandleCache,
}

#[derive(Deserialize, Serialize, Clone, Copy)]
enum FontType {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontType {
    pub fn as_str(&self) -> &str {
        match &self {
            Self::Regular => "Regular",
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::BoldItalic => "Bold-Italic",
        }
    }
}

struct FontInfo {
    handle: Handle,
    family_name: FamilyName,
    font_type: FontType,
}

impl Display for FontInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}",
            Self::family_name_to_str(&self.family_name),
            self.font_type.as_str()
        )
    }
}
impl FontInfo {
    pub fn family_name_to_str(family_name: &FamilyName) -> &str {
        match &family_name {
            FamilyName::SansSerif => "default-sans-serif",
            FamilyName::Monospace => "default-monospace",
            FamilyName::Title(name) => name,
            _ => unreachable!("We don't allow other default font types"),
        }
    }
}

impl FontCache {
    async fn new(name: &str, font_infos: &[FontInfo]) -> anyhow::Result<Self> {
        let mut it = font_infos.iter().map(HandleCache::new);
        let cache = Self {
            name: name.to_owned(),
            base: it.next().context("Font info missing")?.await?,
            italic: it.next().context("Font info missing")?.await?,
            bold: it.next().context("Font info missing")?.await?,
            bold_italic: it.next().context("Font info missing")?.await?,
        };
        Ok(cache)
    }
}

#[derive(Deserialize, Serialize)]
struct HandleCache {
    path: PathBuf,
    binary: bool,
    font_index: u32,
}

impl HandleCache {
    async fn new(font_info: &FontInfo) -> anyhow::Result<Self> {
        let handle = match &font_info.handle {
            Handle::Path { path, font_index } => Self {
                path: path.to_owned(),
                binary: false,
                font_index: *font_index,
            },
            Handle::Memory { bytes, font_index } => {
                let inlyne_cache = dirs::cache_dir()
                    .context("Couldn't find cache dir")?
                    .join("inlyne");
                let file_name = font_info.to_string();
                let cache_file_path = inlyne_cache.join(file_name.as_str());
                tokio::fs::write(cache_file_path, bytes.as_ref())
                    .await
                    .expect("Writing to font handle cache");
                Self {
                    path: file_name.into(),
                    binary: true,
                    font_index: *font_index,
                }
            }
        };
        Ok(handle)
    }
}

impl TryFrom<HandleCache> for Handle {
    type Error = anyhow::Error;
    fn try_from(
        HandleCache {
            path,
            binary,
            font_index,
        }: HandleCache,
    ) -> anyhow::Result<Self> {
        if binary {
            let inlyne_cache = dirs::cache_dir()
                .context("Couldn't find cache dir")?
                .join("inlyne");
            let font_path = inlyne_cache.join(path);
            let bytes = fs::read(font_path)?;

            Ok(Self::Memory {
                bytes: Arc::new(bytes),
                font_index,
            })
        } else {
            Ok(Self::Path { path, font_index })
        }
    }
}

pub async fn get_fonts(font_opts: &FontOptions) -> anyhow::Result<Vec<FontArc>> {
    let regular_name = &font_opts.regular_font;
    let monospace_name = &font_opts.monospace_font;

    if regular_name.is_none() && monospace_name.is_none() {
        load_best_fonts()
    } else {
        match dirs::cache_dir() {
            Some(cache_dir) => {
                let inlyne_cache = cache_dir.join("inlyne");
                if !inlyne_cache.exists() {
                    tokio::fs::create_dir_all(&inlyne_cache)
                        .await
                        .expect("Creating cache directory");
                }

                let reg_cache_path = inlyne_cache.join("font_regular.toml");
                let mut handles = load_maybe_cached_fonts_by_name(
                    regular_name.as_deref(),
                    FamilyName::SansSerif,
                    &reg_cache_path,
                )
                .await?;

                let mono_cache_path = inlyne_cache.join("font_mono.toml");
                let mono_handles = load_maybe_cached_fonts_by_name(
                    monospace_name.as_deref(),
                    FamilyName::Monospace,
                    &mono_cache_path,
                )
                .await?;

                handles.extend(mono_handles.into_iter());
                Ok(handles)
            }
            None => load_best_fonts(),
        }
    }
}

async fn load_maybe_cached_fonts_by_name(
    name: Option<&str>,
    fallback_family: FamilyName,
    path: &Path,
) -> anyhow::Result<Vec<FontArc>> {
    let fonts = match name {
        Some(name) => match load_cached_fonts_by_name(name, path).await {
            Some(fonts) => fonts,
            None => {
                let handles = load_best_handles_by_name(FamilyName::Title(name.to_owned()))?;
                if let Ok(font_cache) = FontCache::new(name, &handles).await {
                    tokio::fs::write(path, toml::to_string(&font_cache)?).await?;
                }
                handles
                    .into_iter()
                    .map(|info| load_font(info.handle))
                    .collect::<anyhow::Result<Vec<_>>>()?
            }
        },
        None => load_best_fonts_by_name(fallback_family)?,
    };
    Ok(fonts)
}

fn load_best_fonts() -> anyhow::Result<Vec<FontArc>> {
    let mut fonts = load_best_fonts_by_name(FamilyName::SansSerif)?;
    fonts.extend(load_best_fonts_by_name(FamilyName::Monospace)?.into_iter());
    Ok(fonts)
}

fn load_best_handles_by_name(family_name: FamilyName) -> anyhow::Result<Vec<FontInfo>> {
    let source = SystemSource::new();
    let name = &[family_name.clone()];
    let base = FontInfo {
        handle: select_best_font(&source, name, Properties::new().style(Style::Normal))?,
        family_name: family_name.clone(),
        font_type: FontType::Regular,
    };
    let italic = FontInfo {
        handle: select_best_font(&source, name, Properties::new().style(Style::Italic))?,
        family_name: family_name.clone(),
        font_type: FontType::Italic,
    };
    let bold = FontInfo {
        handle: select_best_font(&source, name, Properties::new().weight(Weight::BOLD))?,
        family_name: family_name.clone(),
        font_type: FontType::Bold,
    };
    let bold_italic = FontInfo {
        handle: select_best_font(
            &source,
            name,
            Properties::new().weight(Weight::BOLD).style(Style::Italic),
        )?,
        family_name,
        font_type: FontType::BoldItalic,
    };

    Ok(vec![base, italic, bold, bold_italic])
}

fn load_best_fonts_by_name(family_name: FamilyName) -> anyhow::Result<Vec<FontArc>> {
    let handles = load_best_handles_by_name(family_name)?;
    handles
        .into_iter()
        .map(|info| load_font(info.handle))
        .collect::<anyhow::Result<Vec<_>>>()
}

async fn load_cached_fonts_by_name(desired_name: &str, path: &Path) -> Option<Vec<FontArc>> {
    let contents = tokio::fs::read_to_string(path).await.ok()?;
    let FontCache {
        name,
        base,
        italic,
        bold,
        bold_italic,
    } = toml::from_str(&contents).ok()?;

    if desired_name == name {
        [base, italic, bold, bold_italic]
            .into_iter()
            .map(|cached| {
                let handle = Handle::try_from(cached).ok()?;
                load_font(handle).ok()
            })
            .collect::<Option<Vec<_>>>()
    } else {
        None
    }
}

fn select_best_font(
    source: &SystemSource,
    name: &[FamilyName],
    props: &Properties,
) -> anyhow::Result<Handle> {
    source.select_best_match(name, props).with_context(|| {
        format!(
            "No font found for name: {:?} properties: {:#?}",
            name, props
        )
    })
}

fn load_font(handle: Handle) -> anyhow::Result<FontArc> {
    match handle {
        Handle::Path { path, font_index } => {
            let file = fs::File::open(path)?;
            // Font files can be big. Memmap and leak the font file to avoid keeping it in memory.
            // Because we load 8 font files this will leak exactly 8 * 16 = 128 bytes
            // SAFETY: This is safe as long as nothing (either in or outside of this program)
            // modifies the file while it's memmapped. Unfortunately there is nothing we can do to
            // guarantee this won't happen, but memmapping font files is a common practice
            let mmap = unsafe { memmap2::Mmap::map(&file)? };
            let leaked = Box::leak(Box::new(mmap));
            Ok(FontArc::from(FontRef::try_from_slice_and_index(
                leaked, font_index,
            )?))
        }
        Handle::Memory { bytes, font_index } => Ok(FontArc::from(FontVec::try_from_vec_and_index(
            bytes.to_vec(),
            font_index,
        )?)),
    }
}
