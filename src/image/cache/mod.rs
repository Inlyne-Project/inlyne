use std::{path::PathBuf, time::SystemTime};

use crate::image::ImageData;

use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};
use url::Url;

mod global;
mod session;

pub use global::run_garbage_collector as run_global_garbage_collector;

// TODO: spawn a cache worker when creating the cache and return a handle that can communicate with
// it? Each request can be pushed to a thread-pool that shares the cache?

const MAX_CACHE_SIZE_BYTES: u64 = 256 * 1_024 * 1_024;
const MAX_ENTRY_SIZE_BYTES: u64 = MAX_CACHE_SIZE_BYTES / 10;

// Internally stores a URL, but we keep it as a string to simplify DB storage and comparisons
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
struct RemoteKey(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Remote(RemoteKey),
    Local(PathBuf),
}

impl TryFrom<&[u8]> for RemoteKey {
    type Error = anyhow::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(bytes)?;
        Ok(s.into())
    }
}

impl From<&str> for RemoteKey {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<Url> for RemoteKey {
    fn from(url: Url) -> Self {
        Self(url.to_string())
    }
}

impl Key {
    fn from_abs_path(path: PathBuf) -> anyhow::Result<Self> {
        // TODO: check that it's absolute
        Ok(Self::Local(path))
    }

    fn from_url(url: &str) -> anyhow::Result<Self> {
        let url = Url::parse(url)?;
        Ok(url.into())
    }
}

impl From<Url> for Key {
    fn from(url: Url) -> Self {
        if url.scheme() == "file" {
            let path = url.to_file_path().unwrap();
            Self::from_abs_path(path).unwrap()
        } else {
            Self::Remote(url.into())
        }
    }
}

// TODO: need to be able to pass in a fake time source, so that we can reasonably test this
/// A multi-layered image cache
///
/// Uses a fast in-memory cache backed by a secondary global cache
pub struct LayeredCache {
    // The bulk of the data in `ImageData` is wrapped in an `Arc<_>`, so this cache serves as a
    // cheap way to pass out copies of that data
    per_session: session::Cache,
    global: Option<global::Cache>,
}

impl LayeredCache {
    pub fn load() -> anyhow::Result<Self> {
        let cache = match global::Cache::load() {
            Ok(global) => Some(global).into(),
            Err(err) => {
                tracing::warn!(
                    "Failed loading persistent image cache: {err}\nFalling back to in-memory cache"
                );
                None.into()
            }
        };
        Ok(cache)
    }

    /// Create a new cache in-memory
    #[cfg(test)]
    pub fn in_memory() -> Self {
        Some(global::Cache::in_memory()).into()
    }

    pub fn fetch_cached(&mut self, key: Key) -> anyhow::Result<ImageData> {
        let session_cache_policy = match key {
            // Local images are exclusively handled by the per-session cache
            Key::Local(local) => return self.per_session.fetch_local_cached(local),
            Key::Remote(remote) => match self.per_session.check_remote_cache(&remote) {
                Some(session::RemoteEntry::Fresh(image_data)) => return Ok(image_data),
                Some(session::RemoteEntry::Stale(cache_policy)) => Some(cache_policy),
                None => None,
            },
        };

        // TODO: check the table's cache policy and cmp with our own to handle fetching
        todo!();
    }
}

impl From<Option<global::Cache>> for LayeredCache {
    fn from(global: Option<global::Cache>) -> Self {
        Self {
            per_session: Default::default(),
            global,
        }
    }
}
