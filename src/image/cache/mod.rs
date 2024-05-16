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

use std::{
    io,
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime},
};

use crate::{image::ImageData, HistTag};

use lz4_flex::frame::FrameEncoder;
use metrics::histogram;
use serde::{Deserialize, Serialize};
use url::Url;

mod global;
mod request;
mod session;
#[cfg(test)]
mod tests;

pub use global::{
    run_garbage_collector as run_global_garbage_collector, stats as global_stats,
    Stats as GlobalStats, StatsInner as GlobalStatsInner,
};
use request::StandardRequest;

// TODO: spawn a cache worker when creating the cache and return a handle that can communicate with
// it? Each request can be pushed to a thread-pool that shares the cache?

const MAX_CACHE_SIZE_BYTES: u64 = 256 * 1_024 * 1_024;
const MAX_ENTRY_SIZE_BYTES: u64 = MAX_CACHE_SIZE_BYTES / 10;

// Internally stores a URL, but we keep it as a string to simplify DB storage and comparisons
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct RemoteKey(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Remote(RemoteKey),
    Local(PathBuf),
}

impl From<RemoteKey> for Key {
    fn from(v: RemoteKey) -> Self {
        Self::Remote(v)
    }
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

impl From<RemoteKey> for ureq::Request {
    fn from(key: RemoteKey) -> Self {
        let req: StandardRequest = (&key).into();
        (&req).into()
    }
}

impl Key {
    fn from_abs_path(path: PathBuf) -> Option<Self> {
        if path.is_absolute() {
            Some(Self::Local(path))
        } else {
            None
        }
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
            Self::from_abs_path(path).expect("URLs are _always_ absolute paths")
        } else {
            Self::Remote(url.into())
        }
    }
}

#[derive(Debug)]
pub enum StoredImage {
    /// Pre-baked image data ready to be served
    PreDecoded(ImageData),
    /// Compressed SVG text
    ///
    /// SVGs get stored as the original text and rendered on demand instead of being pre-rendered
    /// because the rendering for the same SVG can change depending on different dpi or font info.
    /// This will likely be smaller anyways
    CompressedSvg(Vec<u8>),
}

impl StoredImage {
    pub fn from_svg(svg: &str) -> Self {
        let mut input = io::Cursor::new(svg.as_bytes());
        // TODO: upstream a helper function that does this
        let mut compressor = FrameEncoder::new(Vec::new());
        io::copy(&mut input, &mut compressor).expect("in-memory I/O failed");
        let output = compressor.finish().unwrap();
        Self::CompressedSvg(output)
    }

    fn render(&self, ctx: &SvgContext) -> ImageData {
        todo!();
    }
}

impl From<ImageData> for StoredImage {
    fn from(data: ImageData) -> Self {
        Self::PreDecoded(data)
    }
}

pub trait TimeSource: 'static {
    fn now(&self) -> SystemTime;
}

struct SystemTimeSource;

impl TimeSource for SystemTimeSource {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Our custom `CacheOptions` (could be `const`)
fn cache_options() -> http_cache_semantics::CacheOptions {
    // TODO: PR upstream for `const fn new() -> CacheOptions`
    http_cache_semantics::CacheOptions {
        // Our cache is per-user aka private
        shared: false,
        ..Default::default()
    }
}

#[derive(Clone)]
pub struct LayeredCache(Arc<Inner>);

// TODO: expose the cache through a channel and have a cache manager thread that handles requests

pub struct Inner {
    per_session: session::Cache,
    // No global cache if we can't create one at the expected location
    global: Option<global::Cache>,
    time: Box<dyn TimeSource>,
    svg_ctx: SvgContext,
}

struct SvgContext {
    dpi: f32,
}

impl LayeredCache {
    pub fn load(svg_ctx: SvgContext) -> anyhow::Result<Self> {
        Self::load_with_time(SystemTimeSource, svg_ctx)
    }

    pub fn load_with_time<Time>(time: Time, svg_ctx: SvgContext) -> anyhow::Result<Self> where Time: TimeSource {
        let global = match global::Cache::load() {
            Ok(global) => Some(global),
            Err(err) => {
                tracing::warn!(
                    "Failed loading persistent image cache: {err}\nFalling back to in-memory cache"
                );
                None
            }
        };
        Ok(Self::new(global, time, svg_ctx))
    }

    /// Create a new cache in-memory
    #[cfg(test)]
    pub fn in_memory_with_time<Time>(time: Time, svg_ctx: SvgContext) -> Self where Time: TimeSource {
        Self::new(Some(global::Cache::in_memory()), time, svg_ctx)
    }

    fn new<Time>(global: Option<global::Cache>, time: Time, svg_ctx: SvgContext) -> Self
    where
        Time: TimeSource,
    {
        let inner = Inner {
            per_session: Default::default(),
            global,
            time: Box::new(time),
            svg_ctx,
        };
        Self(Arc::new(inner))
    }

    pub fn fetch<K: Into<Key>>(&self, key: K) -> anyhow::Result<L1Check> {
        let key = key.into();
        let cache_l1_check = match key {
            // Local images are exclusively handled by the per-session cache
            Key::Local(local) => match self.0.per_session.fetch_local_cached(&local) {
                Some(image_data) => image_data.into(),
                None => L1Cont {
                    cache: self.clone(),
                    kind: L1ContKind::FetchLocal(local),
                }
                .into(),
            },
            Key::Remote(remote) => match self.0.per_session.check_remote_cache(&remote) {
                Some(session::RemoteEntry::Fresh(image_data)) => image_data.into(),
                _ => L1Cont {
                    cache: self.clone(),
                    kind: L1ContKind::CheckL2(remote),
                }
                .into(),
            },
        };

        Ok(cache_l1_check)
    }

    // TODO: move to `session` since that's the only layer that handles local images?
    fn fetch_local_image(&self, path: PathBuf) -> anyhow::Result<StoredImage> {
        todo!();
    }

    fn l2_check(&self, key: &RemoteKey) -> anyhow::Result<global::CacheCheck> {
        if let Some(global) = &self.0.global {
            global.check_remote_cache(&key)
        } else {
            let req: StandardRequest = key.into();
            let parts = (&req).into();
            Ok(global::CacheCont::Miss(parts).into())
        }
    }

    fn l2_cont(&self, cont: global::CacheCont) -> anyhow::Result<StoredImage> {
        let data = match cont {
            global::CacheCont::Miss(req_parts) => self.fetch_remote_image(req_parts.into())?,
            global::CacheCont::TryRefresh(_) => todo!(),
        };

        // TODO: store data in l2

        Ok(data)
    }

    // TODO: extract out most of this image loading logic to share with fetching local images
    // TODO: expose image loading failures in a less opaque way
    fn fetch_remote_image(&self, req: ureq::Request) -> anyhow::Result<StoredImage> {
        let start = Instant::now();
        let url = req.url().to_owned();

        let image_data = if let Ok(bytes) = super::http_call_req(req) {
            bytes
        } else {
            tracing::warn!("Request for image {url} failed");
            let image =
                ImageData::load(include_bytes!("../../../assets/img/broken.png"), false).unwrap();
            return Ok(image.into());
        };

        let image = if let Ok(image) = ImageData::load(&image_data, true) {
            image.into()
        } else {
            todo!("Handle SVG stuff");
        };

        histogram!(HistTag::ImageLoad).record(start.elapsed());
        Ok(image)
    }
}

#[must_use]
pub enum L1Check {
    // We are done 🥳🎉
    Fini(ImageData),
    // Needs follow-up
    Cont(L1Cont),
}

impl From<ImageData> for L1Check {
    fn from(image_data: ImageData) -> Self {
        Self::Fini(image_data)
    }
}

impl From<L1Cont> for L1Check {
    fn from(cont: L1Cont) -> Self {
        Self::Cont(cont)
    }
}

#[must_use]
pub struct L1Cont {
    cache: LayeredCache,
    kind: L1ContKind,
}

enum L1ContKind {
    CheckL2(RemoteKey),
    FetchLocal(PathBuf),
}

impl L1Cont {
    pub fn finish(self) -> anyhow::Result<ImageData> {
        let Self { cache, kind } = self;
        let image_date = match kind {
            L1ContKind::CheckL2(remote) => {
                let data = match cache.l2_check(&remote)? {
                    global::CacheCheck::Fresh(image_data) => image_data,
                    global::CacheCheck::Cont(cont) => cache.l2_cont(cont)?.into(),
                }.render(&cache.0.svg_ctx);

                // TODO: store in l1

                data
            }
            L1ContKind::FetchLocal(path) => {
                let data = cache.fetch_local_image(path)?.render(&cache.0.svg_ctx);

                // TODO: store in l1

                data
            }
        };

        Ok(image_date)
    }
}
