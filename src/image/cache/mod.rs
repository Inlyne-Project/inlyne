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
    fmt, io, path::PathBuf, sync::Arc, time::{Instant, SystemTime}
};

use crate::{image::ImageData, HistTag};

use http_cache_semantics::{CachePolicy, RequestLike};
use lz4_flex::frame::FrameEncoder;
use metrics::histogram;
use serde::{Deserialize, Serialize};
use url::Url;

mod global;
// TODO: this shouldn't be pub
pub mod request;
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Remote(RemoteKey),
    Local(PathBuf),
}

// Internally stores a URL, but we keep it as a string to simplify DB storage and comparisons
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct RemoteKey(String);

impl RemoteKey {
    fn new_unchecked<I: Into<String>>(s: I) -> Self {
        Self(s.into())
    }
}

impl From<RemoteKey> for Key {
    fn from(v: RemoteKey) -> Self {
        Self::Remote(v)
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

#[derive(Clone, Debug)]
pub enum StableImage {
    /// Pre-baked image data ready to be served
    PreDecoded(ImageData),
    /// Compressed SVG text
    ///
    /// SVGs get stored as the original text and rendered on demand instead of being pre-rendered
    /// because the rendering for the same SVG can change depending on different dpi or font info.
    /// This will likely be smaller anyways
    CompressedSvg(Vec<u8>),
}

impl StableImage {
    pub fn from_svg(svg: &str) -> Self {
        let mut input = io::Cursor::new(svg.as_bytes());
        // TODO: upstream a helper function that does this
        let mut compressor = FrameEncoder::new(Vec::new());
        io::copy(&mut input, &mut compressor).expect("in-memory I/O failed");
        let output = compressor.finish().unwrap();
        Self::CompressedSvg(output)
    }

    fn render(self, ctx: &SvgContext) -> ImageData {
        match self {
            Self::PreDecoded(data) => data,
            Self::CompressedSvg(compressed) => todo!(),
        }
    }
}

impl From<ImageData> for StableImage {
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

pub struct Shared {
    per_session: session::Cache,
    time: Box<dyn TimeSource>,
    svg_ctx: SvgContext,
}

struct SvgContext {
    dpi: f32,
}

// TODO: restructure how a lot of this is done. Allow for checking the l1 cache without touching a
// db connection, and allow for either a pool of actual workers or an `Arc<Mutex<Connection>>` for
// a shareable in-memory db
#[derive(Clone)]
pub struct LayeredCache(Arc<Shared>);

impl LayeredCache {
    pub fn new(svg_ctx: SvgContext) -> anyhow::Result<Self> {
        Ok(Self::init(SystemTimeSource, svg_ctx))
    }

    #[cfg(test)]
    pub fn new_with_time<T>(
        time: T,
        svg_ctx: SvgContext,
    ) -> anyhow::Result<Self>
    where
        T: TimeSource,
    {
        Ok(Self::init(time, svg_ctx))
    }

    fn init<Time>(time: Time, svg_ctx: SvgContext) -> Self
    where
        Time: TimeSource,
    {
        let shared = Shared {
            per_session: Default::default(),
            time: Box::new(time),
            svg_ctx,
        };
        Self(Arc::new(shared))
    }

    pub fn load(&self, svg_ctx: SvgContext) -> anyhow::Result<LayeredCacheWorker> {
        let global = global::Cache::load()
            .inspect_err(|err| tracing::warn!("Failed loading persistent image cache: {err}"))
            .ok();
        Ok(self.worker(global))
    }

    #[cfg(test)]
    pub fn from_file(
        &self,
        path: &std::path::Path,
    ) -> anyhow::Result<LayeredCacheWorker>
    {
        let global = global::Cache::load_from_file(path)?;
        Ok(self.worker(Some(global)))
    }

    /// Create a new cache in-memory
    #[cfg(test)]
    pub fn in_memory(&self) -> LayeredCacheWorker
    {
        self.worker(None)
    }

    fn worker(&self, global: Option<global::Cache>) -> LayeredCacheWorker {
        let shared = Arc::clone(&self.0);
        LayeredCacheWorker {
            shared,
            global,
        }
    }
}

pub struct LayeredCacheWorker {
    shared: Arc<Shared>,
    // No global cache if we can't create one at the expected location
    global: Option<global::Cache>,
}

impl fmt::Debug for LayeredCacheWorker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LayeredCacheWorker")
    }
}

impl LayeredCacheWorker {
    pub fn fetch<K: Into<Key>>(self, key: K) -> anyhow::Result<L1Check> {
        let key = key.into();
        let cache_l1_check = match key {
            // Local images are exclusively handled by the per-session cache
            Key::Local(local) => match self.shared.per_session.fetch_local_cached(&local) {
                Some(image_data) => image_data.into(),
                None => L1Cont {
                    cache: self,
                    kind: L1ContKind::FetchLocal(local),
                }
                .into(),
            },
            Key::Remote(remote) => match self.shared.per_session.check_remote_cache(&remote) {
                Some(session::RemoteEntry::Fresh(image_data)) => image_data.into(),
                _ => L1Cont {
                    cache: self,
                    kind: L1ContKind::CheckL2(remote),
                }
                .into(),
            },
        };

        Ok(cache_l1_check)
    }

    // TODO: move to `session` since that's the only layer that handles local images?
    fn fetch_local_image(&self, path: PathBuf) -> anyhow::Result<StableImage> {
        todo!()
    }

    fn l2_check(&self, key: &RemoteKey) -> anyhow::Result<global::CacheCheck> {
        if let Some(global) = &self.global {
            global.check_remote_cache(&key)
        } else {
            let req: StandardRequest = key.into();
            let parts = (&req).into();
            Ok(global::CacheCont::Miss(parts).into())
        }
    }

    fn l2_cont(
        &mut self,
        cont: global::CacheCont,
    ) -> anyhow::Result<ImageResult<(CachePolicy, StableImage)>> {
        let (key, image_res) = match cont {
            global::CacheCont::Miss(req_parts) => {
                let url = req_parts.uri();
                let key = RemoteKey::new_unchecked(url.to_string());
                let image_res = self.fetch_remote_image(req_parts.into())?;
                (key, image_res)
            },
            global::CacheCont::TryRefresh(_) => todo!(),
        };

        // TODO: store data in l2
        if let (Some(global), Ok((policy, image))) = (&mut self.global, &image_res) {
            global.insert(&key, policy, image.to_owned())?;
        }

        Ok(image_res)
    }

    // TODO: extract out most of this image loading logic to share with fetching local images
    fn fetch_remote_image(
        &self,
        req: ureq::Request,
    ) -> anyhow::Result<ImageResult<(CachePolicy, StableImage)>> {
        let start = Instant::now();
        let url = req.url().to_owned();
        let standard_req: StandardRequest = url.parse().unwrap();

        let Ok((standard_resp, body)) = request::http_call_req(req) else {
            tracing::warn!("Request for image {url} failed");
            return Ok(Err(ImageError::ReqFailed));
        };
        let policy = CachePolicy::new(&standard_req, &standard_resp);

        let image = if let Ok(image) = ImageData::load(&body, true) {
            image.into()
        } else {
            todo!("Handle SVG stuff");
        };

        histogram!(HistTag::ImageLoad).record(start.elapsed());
        Ok(Ok((policy, image)))
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
    cache: LayeredCacheWorker,
    kind: L1ContKind,
}

enum L1ContKind {
    CheckL2(RemoteKey),
    FetchLocal(PathBuf),
}

impl L1Cont {
    pub fn finish(self) -> anyhow::Result<ImageResult<(LayeredCacheWorker, ImageData)>> {
        let Self { mut cache, kind } = self;
        let image_date = match kind {
            L1ContKind::CheckL2(remote) => {
                let (policy, data) = match cache.l2_check(&remote)? {
                    global::CacheCheck::Fresh((policy, stored_image)) => {
                        (policy, stored_image.render(&cache.shared.svg_ctx))
                    }
                    global::CacheCheck::Cont(cont) => match cache.l2_cont(cont)? {
                        Ok((policy, stored_image)) => {
                            (policy, stored_image.render(&cache.shared.svg_ctx))
                        }
                        Err(e) => return Ok(Err(e)),
                    },
                };

                cache
                    .shared
                    .per_session
                    .insert_remote(remote, (policy, data.clone()));

                data
            }
            L1ContKind::FetchLocal(path) => {
                let data = cache
                    .fetch_local_image(path.clone())?
                    .render(&cache.shared.svg_ctx);

                cache.shared.per_session.insert_local(path, data.clone());

                data
            }
        };

        Ok(Ok((cache, image_date)))
    }
}

type ImageResult<T> = Result<T, ImageError>;

#[derive(Debug, PartialEq, Eq)]
pub enum ImageError {
    ReqFailed,
}
