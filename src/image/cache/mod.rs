use std::{collections::BTreeMap, fs, path::Path, time::SystemTime};

use crate::image::ImageData;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use url::Url;

mod global;

pub use global::run_garbage_collector as run_global_garbage_collector;

// TODO: spawn a cache worker when creating the cache and return a handle that can communicate with
// it?

const MAX_CACHE_SIZE_BYTES: u64 = 256 * 1_024 * 1_024;
const MAX_ENTRY_SIZE_BYTES: u64 = MAX_CACHE_SIZE_BYTES / 10;

// Internally stores a URL, but we keep it as a string to simplify DB storage and comparisons
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Key(String);

impl TryFrom<&[u8]> for Key {
    type Error = anyhow::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(bytes)?;
        Ok(s.into())
    }
}

impl From<&str> for Key {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl Key {
    fn from_abs_path(path: &Path) -> anyhow::Result<Self> {
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

impl From<Url> for Key {
    fn from(url: Url) -> Self {
        Self(url.to_string())
    }
}

#[derive(Debug)]
struct LocalMeta {
    last_used: SystemTime,
    m_time: SystemTime,
}

/// A probe meant to store some short-lived information for checking if cache entries are valid
#[derive(Clone, Copy, Debug)]
pub struct ValidationProbe {
    time: SystemTime,
    source: TimeSource,
}

#[derive(Clone, Copy, Debug)]
pub enum TimeSource {
    LocalMTime,
    Now,
}

impl TryFrom<&Key> for ValidationProbe {
    type Error = anyhow::Error;

    fn try_from(Key(key): &Key) -> Result<Self, Self::Error> {
        let (time, source) = if key.starts_with("file://") {
            // TODO(comsic): could refactor `Key` to avoid having to do all of this. Feels a little
            // weird to keep going back and forth from a string url to a parsed url (lazily cache
            // the parsed url with a oncecell?)
            let url: Url = key.parse().expect("TODO: this is infallible");
            let path = url.to_file_path().expect("TODO: this is infallible");
            let meta = fs::metadata(&path)?;
            let m_time = meta.modified()?;
            (m_time, TimeSource::LocalMTime)
        } else {
            let now = SystemTime::now();
            (now, TimeSource::Now)
        };

        Ok(Self { time, source })
    }
}

// TODO: need to be able to pass in a fake time source, so that we can reasonably test this
/// A multi-layered image cache
///
/// Uses a fast in-memory cache backed by a secondary global cache
pub struct LayeredCache {
    // The bulk of the data in `ImageData` is wrapped in an `Arc<_>`, so this cache serves as a
    // cheap way to pass out copies of that data
    per_session: RwLock<BTreeMap<Key, ImageData>>,
    global: global::Cache,
}

impl LayeredCache {
    pub fn load() -> anyhow::Result<Self> {
        let cache = match global::Cache::load() {
            Ok(global) => global.into(),
            Err(err) => {
                tracing::warn!(
                    "Failed loading persistent image cache: {err}\nFalling back to in-memory cache"
                );
                Self::in_memory()
            }
        };
        Ok(cache)
    }

    /// Create a new cache in-memory (useful for testing)
    pub fn in_memory() -> Self {
        global::Cache::in_memory().into()
    }

    pub fn fetch_cached(&mut self, key: Key) -> anyhow::Result<ImageData> {
        let probe: ValidationProbe = (&key)
            .try_into()
            .expect("TODO: log and fallback to fetching from source");
        let from_local_cache = {
            let local_read = self.per_session.read();
            todo!();
            // local_read
            //     .get(&key)
            //     .and_then(|validated_image| validated_image.validate(probe))
        };

        if let Some(image_data) = from_local_cache {
            return Ok(image_data);
        }

        let image_data = self.global.fetch_cached(&key, probe)?;

        {
            let mut local_write = self.per_session.write();
            todo!();
            // let entry = local_write
            //     .entry(key)
            //     .or_insert_with(|| ValidatedImage::new(image_data.clone(), new_validation.clone()));
            // entry.update_validation(new_validation);
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
