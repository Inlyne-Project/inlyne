use std::{borrow::Cow, collections::BTreeMap, path::Path, sync::RwLock, time::SystemTime};

use crate::image::ImageData;

use serde::{Deserialize, Serialize};
use url::Url;

mod global;
mod headers;

pub use global::run_garbage_collector as run_global_garbage_collector;

const MAX_CACHE_SIZE_BYTES: u64 = 256 * 1_024 * 1_024;
const MAX_ENTRY_SIZE_BYTES: u64 = MAX_CACHE_SIZE_BYTES / 10;

// TODO: switch to separate `Key` and `KeyRef` types?
/// Keys are created from urls and absolute paths that we normalize to urls since that seems to be
/// the sanest way to store paths in a cross-platform way
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key<'a>(Cow<'a, str>);

impl<'a> TryFrom<&'a [u8]> for Key<'a> {
    type Error = anyhow::Error;

    fn try_from(bytes: &'a [u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(bytes)?;
        Ok(s.into())
    }
}

impl<'a> From<&'a str> for Key<'a> {
    fn from(s: &'a str) -> Self {
        Self(Cow::Borrowed(s))
    }
}

impl<'a> Key<'a> {
    fn from_path(path: &Path) -> anyhow::Result<Self> {
        let url = Url::from_file_path(path).map_err(|()| {
            // TODO: copy details from docs on when this can happen
            anyhow::anyhow!(
                "Provided path '{}' can't be converted to a URL",
                path.display()
            )
        })?;
        Ok(url.into())
    }

    fn from_url(url: &str) -> anyhow::Result<Self> {
        let url = Url::parse(url)?;
        Ok(url.into())
    }
}

impl From<Url> for Key<'static> {
    fn from(url: Url) -> Self {
        Self(Cow::Owned(url.to_string()))
    }
}

#[derive(Debug)]
struct ValidatedImage {
    image: ImageData,
    validation: Validation,
}

impl ValidatedImage {
    fn new(image: ImageData, validation: Validation) -> Self {
        Self { image, validation }
    }

    fn validate(&self, probe: &ValidationProbe) -> Option<ImageData> {
        self.validation.is_valid(probe).then(|| self.image.clone())
    }

    fn update_validation(&mut self, new: Validation) {
        self.validation = new;
    }
}

/// A probe meant to store some short-lived information for checking if cache entries are valid
// TODO: store system time and
#[derive(Debug)]
struct ValidationProbe(());

impl<'a> TryFrom<&Key<'a>> for ValidationProbe {
    type Error = anyhow::Error;

    fn try_from(key: &Key<'a>) -> Result<Self, Self::Error> {
        todo!();
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Validation {
    // Used for our LRU logic
    last_used: SystemTime,
    kind: ValidationKind,
}

impl Validation {
    fn is_valid(&self, probe: &ValidationProbe) -> bool {
        todo!();
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum ValidationKind {
    Local(SystemTime),
    RemoteUrl(headers::CacheControlMeta),
}

/// A multi-layered image cache
///
/// Uses a fast in-memory cache backed by a secondary global cache
pub struct LayeredCache {
    // The bulk of the data in `ImageData` is wrapped in an `Arc<_>`, so this cache serves as a
    // cheap way to pass out copies of that data
    per_session: RwLock<BTreeMap<Key<'static>, ValidatedImage>>,
    global: global::Cache,
}

impl LayeredCache {
    pub fn load() -> anyhow::Result<Self> {
        global::Cache::load().map(Into::into)
    }

    /// Create a new cache in-memory (useful for testing)
    pub fn in_memory() -> anyhow::Result<Self> {
        global::Cache::in_memory().map(Into::into)
    }

    pub fn fetch_cached(&mut self, key: Key<'static>) -> anyhow::Result<ImageData> {
        let probe: ValidationProbe = (&key)
            .try_into()
            .expect("TODO: log and fallback to fetching from source");
        let from_local_cache = {
            let local_read = self.per_session.read().expect("TODO");
            local_read
                .get(&key)
                .and_then(|validated_image| validated_image.validate(&probe))
        };

        if let Some(image_data) = from_local_cache {
            return Ok(image_data);
        }

        let (new_validation, image_data) = self.global.fetch_cached(&key, &probe)?;

        {
            let mut local_write = self.per_session.write().expect("TODO");
            let entry = local_write
                .entry(key)
                .or_insert_with(|| ValidatedImage::new(image_data.clone(), new_validation.clone()));
            entry.update_validation(new_validation);
        }

        Ok(image_data)
    }
}

impl From<global::Cache> for LayeredCache {
    fn from(global: global::Cache) -> Self {
        Self {
            per_session: RwLock::default(),
            global,
        }
    }
}
