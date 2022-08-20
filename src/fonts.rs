use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::SystemSource;
use font_kit::sources::fontconfig::FontconfigSource;
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

impl FontCache {
    fn new(name: &str, handles: &[Handle]) -> Option<Self> {
        let mut it = handles.iter().map(HandleCache::new);
        let cache = Self {
            name: name.to_owned(),
            base: it.next().flatten()?,
            italic: it.next().flatten()?,
            bold: it.next().flatten()?,
            bold_italic: it.next().flatten()?,
        };
        Some(cache)
    }
}

#[derive(Deserialize, Serialize)]
struct HandleCache {
    path: PathBuf,
    font_index: u32,
}

impl HandleCache {
    fn new(handle: &Handle) -> Option<Self> {
        if let Handle::Path { path, font_index } = handle {
            Some(Self {
                path: path.to_owned(),
                font_index: *font_index,
            })
        } else {
            None
        }
    }
}

impl From<HandleCache> for Handle {
    fn from(HandleCache { path, font_index }: HandleCache) -> Self {
        Self::Path { path, font_index }
    }
}

pub fn get_fonts(font_opts: &FontOptions) -> anyhow::Result<Vec<FontArc>> {
    let regular_name = &font_opts.regular_font;
    let monospace_name = &font_opts.monospace_font;

    let handles = if regular_name.is_none() && monospace_name.is_none() {
        load_best_handles()?
    } else {
        match dirs::cache_dir() {
            Some(cache_dir) => {
                let inlyne_cache = cache_dir.join("inlyne");
                if !inlyne_cache.exists() {
                    let _ = fs::create_dir_all(&inlyne_cache);
                }

                let reg_cache_path = inlyne_cache.join("font_regular.toml");
                let mut handles = load_maybe_cached_handles_by_name(
                    regular_name.as_deref(),
                    FamilyName::SansSerif,
                    &reg_cache_path,
                )?;

                let mono_cache_path = inlyne_cache.join("font_mono.toml");
                let mono_handles = load_maybe_cached_handles_by_name(
                    monospace_name.as_deref(),
                    FamilyName::Monospace,
                    &mono_cache_path,
                )?;

                handles.extend(mono_handles.into_iter());
                handles
            }
            None => load_best_handles()?,
        }
    };

    handles
        .into_iter()
        .map(load_handle)
        .collect::<anyhow::Result<Vec<FontArc>>>()
}

fn load_maybe_cached_handles_by_name(
    name: Option<&str>,
    fallback_family: FamilyName,
    path: &Path,
) -> anyhow::Result<Vec<Handle>> {
    let handles = match name {
        Some(name) => match load_cached_handles_by_name(name, path) {
            Some(handles) => handles,
            None => {
                let handles = load_best_handles_by_name(FamilyName::Title(name.to_owned()))?;
                if let Some(font_cache) = FontCache::new(name, &handles) {
                    fs::write(path, toml::to_string(&font_cache)?)?;
                }
                handles
            }
        },
        None => load_best_handles_by_name(fallback_family)?,
    };
    Ok(handles)
}

fn load_best_handles() -> anyhow::Result<Vec<Handle>> {
    let mut handles = load_best_handles_by_name(FamilyName::SansSerif)?;
    handles.extend(load_best_handles_by_name(FamilyName::Monospace)?.into_iter());
    Ok(handles)
}

fn load_best_handles_by_name(family_name: FamilyName) -> anyhow::Result<Vec<Handle>> {
    let source = SystemSource::new();
    let name = &[family_name];
    let base = select_best_font(&source, name, Properties::new().style(Style::Normal))?;
    let italic = select_best_font(&source, name, Properties::new().style(Style::Italic))?;
    let bold = select_best_font(&source, name, Properties::new().weight(Weight::BOLD))?;
    let bold_italic = select_best_font(
        &source,
        name,
        Properties::new().weight(Weight::BOLD).style(Style::Italic),
    )?;

    Ok(vec![base, italic, bold, bold_italic])
}

fn load_cached_handles_by_name(desired_name: &str, path: &Path) -> Option<Vec<Handle>> {
    let contents = fs::read_to_string(path).ok()?;
    let FontCache {
        name,
        base,
        italic,
        bold,
        bold_italic,
    } = toml::from_str(&contents).ok()?;

    if desired_name == name {
        let handles = [base, italic, bold, bold_italic]
            .into_iter()
            .map(Handle::from)
            .collect();
        Some(handles)
    } else {
        None
    }
}

fn select_best_font(
    source: &FontconfigSource,
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

pub fn load_handle(handle: Handle) -> anyhow::Result<FontArc> {
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
