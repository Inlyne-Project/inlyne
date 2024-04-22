//! Contains our image caching logic
//!
//! The current cache is a 2-layered cache consisting of a volatile per-session cache along with a
//! persistent per-user cache
//!
//! # Image source
//!
//! `inlyne` can load images from either local files stored on the user's computer, or from images
//! requested from remote URLs
//!
//! ## Local Images (from files)
//!
//! Local images are handled exclusively by the per-session cache since there's no point in taking
//! space from remote images which are much more important in terms of caching
//!
//! Validity is determined by storing and comparing the local file's last modified time where an
//! entry is valid if the last modified time is an exact match
//!
//! ## Remote Images (from URLs)
//!
//! Remote images are stored in all layers of the cache
//!
//! Validity is determined according to the rules codified in the `http-cache-semantics` crate
//! which depends on both the request and response headers. Our actions are determined by the
//! response from the `.before_request()` and `.after_response()` hooks
//!
//! # Cache Layers
//!
//! Like typical layered caches entries are retrieved by going down the layers, pulling the entries
//! up through all of the levels when updating
//!
//! ## L1 - Volatile Per-Session Cache
//!
//! The per-session cache provides 2 key functions:
//!
//! 1. A fast lookup to avoid reaching out to the global database on every request
//!     - Reloading the page should not re-pull all of the images from the database
//!     - The slowest aspects of checking this cache are either waiting for writers on the
//!       `RwLock`s and stating the local file to get its last modified time
//! 2. The ability to make cheap copies of image data
//!     - The bulk of the data is stored in `Arc<_>`s which are cheap to copy
//!
//! ## L2 - Persistent Per-User Cache
//!
//! The persistent per-user cache functions as a typical private HTTP cache. This affords most of
//! the typical benefits of an HTTP cache e.g. avoiding making requests on fresh content, avoiding
//! re-transferring bodies on matching E-Tags, etc.
//!
//! # Garbage Collection
//!
//! Entries are evicted based on both a global size limit and a global time-to-live (TTL).
//! Constraining along both of these allows for the cache to behave well for both very active and
//! inactive users. Active users can sit at the cache size limit assuming they look at enough
//! images often enough to fully saturate the cache to the size limit. Inactive users can have a
//! smaller cache as only the entries that are within the global TTL will be retained

use std::{path::PathBuf, str::FromStr, sync::OnceLock, time::SystemTime};

use crate::image::ImageData;

use http::{header, request, HeaderMap};
use http_cache_semantics::{BeforeRequest, CachePolicy, RequestLike};
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

/// Represents the very basic request parts that we always use
#[derive(Clone, Debug)]
struct StandardRequest {
    url: http::Uri,
}

impl From<&RemoteKey> for StandardRequest {
    fn from(key: &RemoteKey) -> Self {
        key.0
            .parse()
            .expect("Remote key should always be a valid url")
    }
}

impl FromStr for StandardRequest {
    type Err = http::uri::InvalidUri;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = s.parse()?;
        Ok(Self { url })
    }
}

impl From<&StandardRequest> for ureq::Request {
    fn from(standard_req: &StandardRequest) -> Self {
        ureq::get(&standard_req.url.to_string())
    }
}

impl RequestLike for StandardRequest {
    fn uri(&self) -> http::Uri {
        self.url.clone()
    }

    fn method(&self) -> &http::Method {
        &http::Method::GET
    }

    fn headers(&self) -> &'static http::HeaderMap {
        static HEADERS: OnceLock<HeaderMap> = OnceLock::new();
        const DESCRIPTIVE_USER_AGENT: &str = concat!(
            "inlyne ",
            env!("CARGO_PKG_VERSION"),
            " https://github.com/trimental/inlyne"
        );
        HEADERS.get_or_init(|| {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::USER_AGENT,
                header::HeaderValue::from_static(DESCRIPTIVE_USER_AGENT),
            );
            headers
        })
    }

    fn is_same_uri(&self, other: &http::Uri) -> bool {
        &self.url == other
    }
}

#[must_use]
pub enum L1Check {
    Fini(ImageData),
    Cont(SlowL1Cont),
}

impl From<SlowL1Cont> for L1Check {
    fn from(cont: SlowL1Cont) -> Self {
        Self::Cont(cont)
    }
}

#[must_use]
pub enum SlowL1Cont {
    CheckRemoteCache(RemoteKey),
    FetchLocal(PathBuf),
    FetchRemote(ureq::Request),
}

impl From<ImageData> for L1Check {
    fn from(image_data: ImageData) -> Self {
        Self::Fini(image_data)
    }
}

// TODO: need to be able to pass in a fake time source, so that we can reasonably test this
pub struct LayeredCache {
    per_session: session::Cache,
    // No global cache if we can't create one at the expected location
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

    pub fn quick_l1_check(&self, key: Key) -> anyhow::Result<L1Check> {
        let cache_l1_check = match key {
            // Local images are exclusively handled by the per-session cache
            Key::Local(local) => match self.per_session.fetch_local_cached(&local) {
                Some(image_data) => image_data.into(),
                None => SlowL1Cont::FetchLocal(local).into(),
            },
            Key::Remote(remote) => match self.per_session.check_remote_cache(&remote) {
                Some(session::RemoteEntry::Fresh(image_data)) => image_data.into(),
                Some(session::RemoteEntry::Stale(_)) => SlowL1Cont::CheckRemoteCache(remote).into(),
                /* match &self.global {
                    Some(global) => match global.check_remote_cache(&remote)? {
                        // TODO: this should refresh the per-session cache
                        Some(global::CacheCheck::Fresh(image_data)) => image_data.into(),
                        Some(global::CacheCheck::Stale(req_parts)) => Some(req_parts).into(),
                        None => Some(req_parts).into(),
                    },
                    None => None.into(),
                },
                    */
                None => SlowL1Cont::FetchRemote(todo!()).into(),
            },
        };

        Ok(cache_l1_check)
    }

    pub fn slow_l1_cont(&self, cont: SlowL1Cont) -> anyhow::Result<ImageData> {
        let image_date = match cont {
            SlowL1Cont::CheckRemoteCache(remote) => match &self.global {
                Some(global) => match global.check_remote_cache(&remote)? {
                    // TODO: update the per-session cache
                    Some(global::CacheCheck::Fresh(image_data)) => image_data,
                    Some(global::CacheCheck::Stale(req_parts)) => {
                        self.slow_l1_cont(SlowL1Cont::FetchRemote(req_parts.into()))?
                    }
                    None => self.slow_l1_cont(SlowL1Cont::FetchRemote(todo!()))?,
                },
                None => todo!(),
            },
            SlowL1Cont::FetchLocal(_) => todo!(),
            SlowL1Cont::FetchRemote(_) => todo!(),
        };

        Ok(image_date)
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
