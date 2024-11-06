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
    fmt,
    io::{self, Read},
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime},
};

use crate::{
    image::{ImageBuffer, ImageData},
    HistTag,
};

use http_cache_semantics::{AfterResponse, CachePolicy, RequestLike};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};
use metrics::histogram;
use resvg::{tiny_skia, usvg};
use serde::{Deserialize, Serialize};
use url::Url;

mod global;
// TODO: this shouldn't be pub
pub mod request;
mod session;
#[cfg(test)]
mod tests;

pub use global::{
    run_garbage_collector as run_global_garbage_collector, Stats as GlobalStats,
    StatsInner as GlobalStatsInner,
};
use request::StandardRequest;

// TODO: spawn a cache worker when creating the cache and return a handle that can communicate with
// it? Each request can be pushed to a thread-pool that shares the cache?

const MAX_CACHE_SIZE_BYTES: u64 = 256 * 1_024 * 1_024;

fn load_image(bytes: &[u8]) -> anyhow::Result<StableImage> {
    let image = if let Ok(image) = ImageData::load(&bytes, true) {
        image.into()
    } else {
        // TODO: how to verify that this is an svg?
        let svg = std::str::from_utf8(bytes)?;
        StableImage::from_svg(&svg)
    };
    Ok(image)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Remote(RemoteKey),
    Local(PathBuf),
}

// Internally stores a URL, but we keep it as a string to simplify DB storage and comparisons
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct RemoteKey(String);

impl fmt::Display for RemoteKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl RemoteKey {
    pub fn new_unchecked<I: Into<String>>(s: I) -> Self {
        Self(s.into())
    }

    pub fn get(&self) -> &str {
        &self.0
    }
}

impl From<RemoteKey> for Key {
    fn from(key: RemoteKey) -> Self {
        Self::Remote(key)
    }
}

impl From<&RemoteKey> for Key {
    fn from(key: &RemoteKey) -> Self {
        key.to_owned().into()
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

impl From<&Key> for Key {
    fn from(key_ref: &Key) -> Self {
        key_ref.to_owned()
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

    pub fn render(self, ctx: &SvgContext) -> ImageResult<ImageData> {
        match self {
            Self::PreDecoded(data) => Ok(data),
            Self::CompressedSvg(compressed) => {
                let mut svg_bytes = Vec::with_capacity(compressed.len());
                let mut decompressor = FrameDecoder::new(io::Cursor::new(compressed));
                decompressor
                    .read_to_end(&mut svg_bytes)
                    .map_err(|_| ImageError::SvgDecompressionError)?;

                let opt = usvg::Options::default();
                // TODO: loading the fontdb on every single SVG render is gonna be slow
                let mut fontdb = usvg::fontdb::Database::new();
                fontdb.load_system_fonts();
                let mut tree = usvg::Tree::from_data(&svg_bytes, &opt)?;
                // TODO: need to check and see if someone can pass a negative dpi and see what kind
                // of issues it can cause
                tree.size = tree.size.scale_to(
                    tiny_skia::Size::from_wh(
                        tree.size.width() * ctx.dpi,
                        tree.size.height() * ctx.dpi,
                    )
                    .ok_or(ImageError::SvgInvalidDimensions)?,
                );
                tree.postprocess(Default::default(), &fontdb);
                let mut pixmap =
                    tiny_skia::Pixmap::new(tree.size.width() as u32, tree.size.height() as u32)
                        .ok_or(ImageError::SvgInvalidDimensions)?;
                resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
                let image_buffer =
                    ImageBuffer::from_raw(pixmap.width(), pixmap.height(), pixmap.data().into())
                        .ok_or(ImageError::SvgContainerTooSmall)?;
                Ok(ImageData::new(image_buffer, false))
            }
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

// TODO: ban typical way of constructing to force usage of vv
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

#[derive(Clone)]
pub struct SvgContext {
    dpi: f32,
}

impl Default for SvgContext {
    fn default() -> Self {
        Self { dpi: 1.0 }
    }
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
    pub fn new_with_time<T>(time: T, svg_ctx: SvgContext) -> anyhow::Result<Self>
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

    pub fn load(&self) -> LayeredCacheWorker {
        let global = global::Cache::load()
            .inspect_err(|err| tracing::warn!("Failed loading persistent image cache: {err}"))
            .ok();
        self.worker(global)
    }

    fn worker(&self, global: Option<global::Cache>) -> LayeredCacheWorker {
        let shared = Arc::clone(&self.0);
        LayeredCacheWorker { shared, global }
    }
}

pub struct LayeredCacheWorker {
    shared: Arc<Shared>,
    // No global cache if we can't create one at the expected location
    global: Option<global::Cache>,
}

impl fmt::Debug for LayeredCacheWorker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LayeredCacheWorker { .. }")
    }
}

impl LayeredCacheWorker {
    pub fn fetch<K: Into<Key>>(self, key: K) -> anyhow::Result<L1Check> {
        let key = key.into();
        let now = self.shared.time.now();
        let session_cache = &self.shared.per_session;
        let cache_l1_check = match key {
            // Local images are exclusively handled by the per-session cache
            Key::Local(local) => match session_cache.fetch_local_cached(&local) {
                Some(image_data) => image_data.into(),
                None => L1Cont {
                    cache: self,
                    kind: L1ContKind::FetchLocal(local),
                }
                .into(),
            },
            Key::Remote(remote) => match session_cache.check_remote_cache(&remote, now) {
                Some(session::RemoteEntry::Fresh(image_data)) => image_data.into(),
                None | Some(session::RemoteEntry::Stale) => L1Cont {
                    cache: self,
                    kind: L1ContKind::CheckL2(remote),
                }
                .into(),
            },
        };

        Ok(cache_l1_check)
    }

    fn l2_check(&self, key: &RemoteKey) -> anyhow::Result<global::CacheCheck> {
        if let Some(global) = &self.global {
            let now = self.shared.time.now();
            global.check_remote_cache(&key, now)
        } else {
            let req: StandardRequest = key.into();
            let parts = (&req).into();
            Ok(global::CacheCont::Miss(parts).into())
        }
    }

    fn l2_cont(
        &mut self,
        cont: global::CacheCont,
    ) -> anyhow::Result<ImageResult<(CachePolicy, ImageSrc, StableImage)>> {
        let (key, image_src, image_res) = match cont {
            global::CacheCont::Miss(req_parts) => {
                let url = req_parts.uri();
                let key = RemoteKey::new_unchecked(url.to_string());
                let image_res = self.fetch_remote_image(req_parts.into())?;
                (key, ImageSrc::RemoteFromSrc, image_res)
            }
            global::CacheCont::TryRefresh((policy, req_parts, stored_image)) => {
                let url = req_parts.uri();
                let req: ureq::Request = req_parts.into();
                let standard_req: StandardRequest = req.url().parse().unwrap();
                let key = RemoteKey::new_unchecked(url.to_string());
                let Ok((standard_resp, body)) = request::http_call_req(req) else {
                    return Ok(Err(ImageError::ReqFailed));
                };

                let now = self.shared.time.now();
                match policy.after_response(&standard_req, &standard_resp, now) {
                    AfterResponse::NotModified(policy, _) => {
                        (key, ImageSrc::L2Refreshed, Ok((policy, stored_image)))
                    }
                    AfterResponse::Modified(policy, _) => {
                        let image = load_image(&body)?;
                        (key, ImageSrc::RemoteFromSrc, Ok((policy, image)))
                    }
                }
            }
        };

        // NIT: this re-stores the image data even on etag refreshes when it could just update the
        // cache policy and lru time instead
        if let (Some(global), Ok((policy, image))) = (&mut self.global, &image_res) {
            if policy.is_storable() {
                let now = self.shared.time.now();
                global.insert(&key, policy, image.to_owned(), now)?;
            }
        }

        Ok(image_res.map(|(policy, stable)| (policy, image_src, stable)))
    }

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
        let now = self.shared.time.now();
        let policy = CachePolicy::new_options(&standard_req, &standard_resp, now, cache_options());

        let image = load_image(&body)?;

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

pub enum ImageSrc {
    L2Fresh,
    L2Refreshed,
    LocalFromSrc,
    RemoteFromSrc,
}

impl L1Cont {
    pub fn finish(self) -> anyhow::Result<ImageResult<(LayeredCacheWorker, ImageSrc, ImageData)>> {
        let Self { mut cache, kind } = self;
        let (image_src, image_date) = match kind {
            L1ContKind::CheckL2(remote) => {
                let (policy, image_src, stored_image) = match cache.l2_check(&remote)? {
                    global::CacheCheck::Fresh((worker, data)) => (worker, ImageSrc::L2Fresh, data),
                    global::CacheCheck::Cont(cont) => match cache.l2_cont(cont)? {
                        Ok(triplet) => triplet,
                        Err(e) => return Ok(Err(e)),
                    },
                };
                let data = match stored_image.render(&cache.shared.svg_ctx) {
                    Ok(data) => data,
                    Err(image_err) => return Ok(Err(image_err)),
                };

                if policy.is_storable() {
                    cache
                        .shared
                        .per_session
                        .insert_remote(remote, (policy, data.clone()));
                }

                (image_src, data)
            }
            L1ContKind::FetchLocal(path) => {
                let (m_time, image) = cache.shared.per_session.fetch_local(&path)?;
                let data = match image.render(&cache.shared.svg_ctx) {
                    Ok(data) => data,
                    Err(image_err) => return Ok(Err(image_err)),
                };

                cache
                    .shared
                    .per_session
                    .insert_local(path, (m_time, data.clone()));

                (ImageSrc::LocalFromSrc, data)
            }
        };

        Ok(Ok((cache, image_src, image_date)))
    }
}

type ImageResult<T> = Result<T, ImageError>;

#[derive(Debug, PartialEq, Eq)]
pub enum ImageError {
    SvgDecompressionError,
    // TODO(cosmic): upstream PR to impl `PartialEq` and `Eq` for `usvg::Error` then include the
    // error in the variant
    InvalidSvg,
    SvgContainerTooSmall,
    SvgInvalidDimensions,
    ReqFailed,
}

impl From<usvg::Error> for ImageError {
    fn from(_: usvg::Error) -> Self {
        Self::InvalidSvg
    }
}
