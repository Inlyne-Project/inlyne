use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    thread::sleep,
    time::{Duration, SystemTime},
};

use super::{
    global::{self, wrappers::StableImageBytes},
    ImageError, ImageSrc, Key, L1Check, LayeredCache, StableImage, SvgContext, TimeSource,
};
use crate::{
    image::ImageData,
    test_utils::{
        image::{Sample, SampleGif, SampleJpg, SamplePng, SampleQoi, SampleSvg, SampleWebp},
        log,
        server::{self, CacheControl},
        temp,
    },
};

use parking_lot::RwLock;
use tempfile::{NamedTempFile, TempDir};

fn touch(file: &Path) {
    let now = filetime::FileTime::now();
    filetime::set_file_mtime(file, now).unwrap();
}

fn cache_control() -> CacheControl {
    CacheControl::new()
}

#[derive(Clone)]
struct FakeTimeSource(Arc<RwLock<SystemTime>>);

impl FakeTimeSource {
    fn inc(&self, delta: Duration) {
        *self.0.write() += delta;
    }
}

impl TimeSource for FakeTimeSource {
    fn now(&self) -> SystemTime {
        *self.0.read()
    }
}

impl Default for FakeTimeSource {
    fn default() -> Self {
        SystemTime::UNIX_EPOCH.into()
    }
}

impl From<SystemTime> for FakeTimeSource {
    fn from(time: SystemTime) -> Self {
        Self(RwLock::new(time).into())
    }
}

fn num_l2_entries(db_path: &Path) -> u32 {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    let num_entries = conn
        .query_row("select count(1) from images", [], |row| row.get(0))
        .unwrap();
    num_entries
}

// TODO: drop for directly using `server::File` instead?
#[derive(Clone)]
struct RemoteImage {
    cache_control: Option<server::CacheControl>,
    content_type: server::ContentType,
    include_etag: bool,
    body: Vec<u8>,
}

impl RemoteImage {
    fn cache_control(mut self, c_c: CacheControl) -> Self {
        self.cache_control = Some(c_c);
        self
    }

    fn include_etag(mut self) -> Self {
        self.include_etag = true;
        self
    }
}

impl From<Sample> for RemoteImage {
    fn from(sample: Sample) -> Self {
        Self {
            cache_control: None,
            content_type: sample.into(),
            body: sample.pre_decode().into(),
            include_etag: false,
        }
    }
}

impl From<SamplePng> for RemoteImage {
    fn from(sample_png: SamplePng) -> Self {
        Sample::from(sample_png).into()
    }
}

impl From<SamplePng> for server::File {
    fn from(sample_png: SamplePng) -> Self {
        RemoteImage::from(sample_png).into()
    }
}

impl From<RemoteImage> for server::File {
    fn from(image: RemoteImage) -> Self {
        let RemoteImage {
            cache_control,
            content_type,
            include_etag,
            body,
        } = image;
        Self {
            mime: content_type,
            cache_control,
            include_etag,
            bytes: body,
        }
    }
}

fn image_server() -> server::MiniServerHandle {
    server::mock_file_server(vec![])
}

const IMMUTABLE_C_C: server::CacheControl = server::CacheControl::new().immutable();
// 300 seconds is used for a lot of images hosted by github
const COMMON_MAX_AGE: Duration = Duration::from_secs(300);

fn cache_builder() -> CacheBuilder {
    Default::default()
}

#[derive(Clone, Default)]
struct CacheBuilder {
    time: Option<FakeTimeSource>,
    svg_ctx: SvgContext,
    max_size: Option<usize>,
    // TODO: vv
    // global_deny_localhost: bool,
}

impl CacheBuilder {
    fn time(mut self, time: FakeTimeSource) -> Self {
        self.time = Some(time);
        self
    }

    fn svg_ctx(mut self, ctx: SvgContext) -> Self {
        self.svg_ctx = ctx;
        self
    }

    fn max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self
    }

    fn l1_only(self) -> TestCache {
        self.finish(WorkerSrc::L1Only)
    }

    fn open_in(self, dir: &Path) -> TestCache {
        let db_path = dir.join(global::db_name());
        self.finish(WorkerSrc::L2Path(db_path))
    }

    fn temp_file(self) -> (TempDir, TestCache) {
        let (tmp_dir, tmp_path) = temp::dir();
        let test_cache = self.open_in(&tmp_path);
        (tmp_dir, test_cache)
    }

    fn finish(self, src: WorkerSrc) -> TestCache {
        let Self {
            time,
            svg_ctx,
            max_size,
        } = self;

        if let Some(max) = max_size {
            todo!();
        }

        let cache = match time {
            Some(fake_time) => LayeredCache::new_with_time(fake_time, svg_ctx),
            None => LayeredCache::new(svg_ctx),
        }
        .unwrap();

        TestCache { cache, src }
    }
}

#[derive(Clone)]
struct TestCache {
    cache: LayeredCache,
    src: WorkerSrc,
}

#[derive(Clone)]
enum WorkerSrc {
    L1Only,
    L2Path(PathBuf),
}

impl TestCache {
    fn path(&self) -> Option<&Path> {
        match &self.src {
            WorkerSrc::L1Only => None,
            WorkerSrc::L2Path(path) => Some(path),
        }
    }

    fn from_l1<K: Into<Key>>(&mut self, key: K) -> Result<ImageData, Fetch> {
        match self.fetch(key) {
            Fetch::L1(data) => Ok(data),
            other => Err(other),
        }
    }

    fn from_l2<K: Into<Key>>(&mut self, key: K) -> Result<ImageData, Fetch> {
        match self.fetch(key) {
            Fetch::L2Fresh(data) => Ok(data),
            other => Err(other),
        }
    }

    fn from_refresh<K: Into<Key>>(&mut self, key: K) -> Result<ImageData, Fetch> {
        match self.fetch(key) {
            Fetch::L2Refreshed(data) => Ok(data),
            other => Err(other),
        }
    }

    fn from_local_src<K: Into<Key>>(&mut self, key: K) -> Result<ImageData, Fetch> {
        match self.fetch(key) {
            Fetch::LocalFromSrc(data) => Ok(data),
            other => Err(other),
        }
    }

    fn from_remote_src<K: Into<Key>>(&mut self, key: K) -> Result<ImageData, Fetch> {
        match self.fetch(key) {
            Fetch::RemoteFromSrc(data) => Ok(data),
            other => Err(other),
        }
    }

    fn err<K: Into<Key>>(&mut self, key: K) -> Result<ImageError, Fetch> {
        match self.fetch(key) {
            Fetch::Err(e) => Ok(e),
            other => Err(other),
        }
    }

    // TODO: refactor so that checking L1 doesn't take a DB connection and then there isn't the
    // consumption of the worker due to needing to take it for `L1Cont`
    fn fetch<K: Into<Key>>(&mut self, key: K) -> Fetch {
        let worker = match &self.src {
            WorkerSrc::L1Only => self.cache.worker(None),
            WorkerSrc::L2Path(path) => {
                let l2_db = global::Cache::load_from_file(path).unwrap();
                self.cache.worker(Some(l2_db))
            }
        };
        match worker.fetch(key.into()).unwrap() {
            L1Check::Fini(data) => Fetch::L1(data),
            L1Check::Cont(cont) => match cont.finish().unwrap() {
                Ok((_, src, data)) => match src {
                    ImageSrc::L2Fresh => Fetch::L2Fresh(data),
                    ImageSrc::L2Refreshed => Fetch::L2Refreshed(data),
                    ImageSrc::LocalFromSrc => Fetch::LocalFromSrc(data),
                    ImageSrc::RemoteFromSrc => Fetch::RemoteFromSrc(data),
                },
                Err(err) => Fetch::Err(err),
            },
        }
    }
}

#[derive(Debug)]
enum Fetch {
    L1(ImageData),
    L2Fresh(ImageData),
    L2Refreshed(ImageData),
    LocalFromSrc(ImageData),
    RemoteFromSrc(ImageData),
    Err(ImageError),
}

fn create_local_image(sample: Sample) -> (NamedTempFile, Key) {
    let (image_file, image_path) = temp::file_with_suffix(sample.suffix());
    let image_bytes = sample.pre_decode();
    fs::write(&image_path, image_bytes).unwrap();
    let key = Key::from_abs_path(image_path).expect("Path is internally canonicalized");
    (image_file, key)
}

// Ensures that we can fetch a remote image from each layer of the cache. Remote images are stored
// in all layers of the cache
#[test]
fn remote_layers() {
    log::init();

    // Setup server
    let server = image_server();
    let sample: Sample = SamplePng::Bun.into();
    let expected_data = sample.post_decode(&Default::default());
    let key = server.mount_image(RemoteImage::from(sample).cache_control(IMMUTABLE_C_C));

    // Setup cache
    let (_tmp_dir, db_path) = temp::dir();
    let mut cache = cache_builder().open_in(&db_path);
    // Fetch from remote and populate all of the cache layers in the process
    let data = cache.from_remote_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");
    // Fetch from l1
    let data = cache.from_l1(&key).expect("L1 is populated");
    assert_eq!(data, expected_data, "Invalid L1 image");
    // Fetch from l2
    let mut empty_l1_cache = cache_builder().open_in(&db_path);
    let data = empty_l1_cache.from_l2(&key).expect("L2 is populated");
    assert_eq!(data, expected_data, "Invalid L2 image");
}

// Local images are only stored in the in-memory cache since the global cache is exclusively for
// remote images
#[test]
fn local_layers() {
    log::init();

    // Local image
    let image: Sample = SampleGif::AtuinDemo.into();
    let (_tmp_image, key) = create_local_image(image);
    let expected_data = image.post_decode(&Default::default());
    // Populate cache
    let mut cache = cache_builder().l1_only();
    let data = cache.from_local_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");
    // And now we can fetch from l1
    let data = cache.from_l1(&key).expect("L1 is populated");
    assert_eq!(data, expected_data, "Invalid L1 image");
}

// SVGs are a special case since they're stored pre-rendered since the rendering can change session
// to session based on the current dpi and available fonts
//
// Remote SVGs are still stored in all layers of the cache
// TODO(cosmic): rendering can change within a session too by doing things like zooming. We should
// really allow for rerendering within a session too
#[test]
fn remote_svg_layers() {
    log::init();

    // Setup server
    let server = image_server();
    let sample: Sample = SampleSvg::Cargo.into();
    let expected_data = sample.post_decode(&Default::default());
    let key = server.mount_image(RemoteImage::from(sample).cache_control(IMMUTABLE_C_C));

    // Setup cache
    let (_tmp_dir, db_path) = temp::dir();
    let cache_builder = cache_builder();
    let mut cache = cache_builder.clone().open_in(&db_path);

    // Fetch from remote and populate all of the cache layers in the process
    let data = cache.from_remote_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");

    // Fetch from l2
    let mut empty_l1_cache = cache_builder.clone().open_in(&db_path);
    let data = empty_l1_cache.from_l2(&key).expect("L2 is populated");
    assert_eq!(data, expected_data, "Invalid L2 image");

    // Try fetching again with different DPI and make sure the rendering changes
    let hidpi_ctx = SvgContext { dpi: 2.0 };
    let hidpi_expected = sample.post_decode(&hidpi_ctx);
    let mut hidpi_cache = cache_builder.svg_ctx(hidpi_ctx).open_in(&db_path);
    let hidpi_data = hidpi_cache.from_l2(&key).expect("L2 has stable SVG");
    assert_eq!(hidpi_data, hidpi_expected, "Bad higher dpi fetch");
    assert_ne!(hidpi_data, data, "Rendering changes with different dpi");
}

// Same as remote SVGs, but not stored in L2 like other local images
#[test]
fn local_svg_layers() {
    log::init();

    // Setup local image
    let image: Sample = SampleSvg::Corro.into();
    let (_tmp_image, key) = create_local_image(image);
    let expected_data = image.post_decode(&Default::default());
    // Setup cache
    let mut cache = cache_builder().l1_only();
    // Fetch from source
    let data = cache.from_local_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");
    // And now from l1
    let data = cache.from_l1(&key).expect("L1 is populated");
    assert_eq!(data, expected_data, "Invalid L1 image");
}

#[test]
fn past_max_age_refetch() {
    log::init();

    let server = image_server();
    let sample: Sample = SampleWebp::CargoPublicApi.into();
    let expected_data = sample.post_decode(&Default::default());
    let c_c = cache_control().max_age(COMMON_MAX_AGE);
    let key = server.mount_image(RemoteImage::from(sample).cache_control(c_c));

    let time = FakeTimeSource::default();
    let (_db_dir, mut cache) = cache_builder().time(time.clone()).temp_file();

    let data = cache.from_remote_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");
    cache.from_l1(&key).expect("Fresh cache");
    time.inc(COMMON_MAX_AGE + Duration::from_secs(1));
    let data = cache
        .from_remote_src(&key)
        .expect("Entry went past max-age and has no e-tag refresh with");
    assert_eq!(data, expected_data, "Refetch should return the same");
}

// TODO: swap out the local file and check that
// Local invalidation is handled by checking against the file's last modified time
#[test]
fn local_invalidation() {
    log::init();

    // Local image
    let image: Sample = SampleJpg::Rgb8.into();
    let (tmp_image, key) = create_local_image(image);
    let expected_data = image.post_decode(&Default::default());

    // Local images are only stored in the l1 cache
    let mut cache = cache_builder().l1_only();
    // Fetch from source
    let data = cache.from_local_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");
    // It's available in l1
    cache.from_l1(&key).expect("L1 is populated");
    // TODO: store the file size as well so we have more granularity than seconds. Then change this
    // to swapping the file for something else and only try fetching once
    // Updating the m-time will invalidate the cached l1 entry
    let refetched = (0..20).find_map(|_| {
        touch(tmp_image.path());
        sleep(Duration::from_millis(50));
        cache.from_local_src(&key).ok()
    });
    assert_eq!(
        refetched,
        Some(data),
        "Touching local image should invalidate l1 and refetch the same image from source",
    );
}

// Stale cache entries with an ETag can use a flow to re-validate the stale entry without having to
// send it from the remote server again. This involves sending our stored e-tag in a special
// request header and the server can respond saying that it still matches the remote entry without
// actually sending the remote entry
#[test]
fn etag_refresh_same() {
    log::init();

    let server = image_server();
    let sample: Sample = SampleQoi::Rgb8.into();
    let c_c = cache_control().max_age(COMMON_MAX_AGE);
    let key = server.mount_image(RemoteImage::from(sample).cache_control(c_c).include_etag());

    let time = FakeTimeSource::default();
    let (_db_dir, mut cache) = cache_builder().time(time.clone()).temp_file();

    cache.from_remote_src(&key).expect("Empty cache");
    cache.from_l1(&key).expect("Fresh cache");
    time.inc(COMMON_MAX_AGE + Duration::from_secs(1));
    cache.from_refresh(&key).expect("Entry went past max-age");
}

// Same as valid e-tag refresh, but this time the content is different and needs to be re-pulled
#[test]
fn etag_refresh_different() {
    log::init();

    let server = image_server();
    let sample: Sample = SampleQoi::Rgb8.into();
    let c_c = cache_control().max_age(COMMON_MAX_AGE);
    let key = server.mount_image(RemoteImage::from(sample).cache_control(c_c).include_etag());

    let time = FakeTimeSource::default();
    let (_db_dir, mut cache) = cache_builder().time(time.clone()).temp_file();

    cache.from_remote_src(&key).expect("Empty cache");
    cache.from_l1(&key).expect("Fresh cache");

    server.swap_image(&key, SamplePng::Ariadne).unwrap();
    time.inc(COMMON_MAX_AGE + Duration::from_secs(1));
    cache
        .from_remote_src(&key)
        .expect("Cached entry is both stale and different now");
}

#[test]
fn stats() {
    fn deterministic_cache_stats(cache: &TestCache) -> String {
        let cache_path = cache.path().unwrap();
        let stats: global::Stats = cache_path.to_owned().try_into().unwrap();
        let stats = stats.to_string();
        stats.replacen(&cache_path.display().to_string(), "<CACHE_PATH>", 1)
    }

    log::init();

    // Setup server
    let server = image_server();
    let png: Sample = SamplePng::Bun.into();
    let png = RemoteImage::from(png)
        .cache_control(cache_control().max_age(COMMON_MAX_AGE))
        .include_etag();
    let png_key = server.mount_image(png);
    let svg: Sample = SampleSvg::Corro.into();
    let svg_key = server.mount_image(RemoteImage::from(svg).cache_control(IMMUTABLE_C_C));

    // Setup cache
    let time = FakeTimeSource::default();
    let (_db_dir, mut cache) = cache_builder().time(time.clone()).temp_file();

    insta::assert_snapshot!(deterministic_cache_stats(&cache), @"path (not found): <CACHE_PATH>");

    cache.fetch(&png_key);
    time.inc(Duration::from_secs(1));
    cache.fetch(&svg_key);

    insta::assert_snapshot!(deterministic_cache_stats(&cache), @r###"
    path: <CACHE_PATH>
    total size: 36 KiB
    "###);
}

// When the cache is over capacity entries will be evicted in order of those that were least
// recently used (LRU)
#[test]
#[ignore = "TODO: waiting for garbage collection"]
fn lru() {
    fn stored_image_data_len(data: ImageData) -> usize {
        stable_image_data_len(data.into())
    }

    fn stable_image_data_len(stable: StableImage) -> usize {
        let bytes: StableImageBytes = stable.into();
        bytes.len()
    }

    log::init();

    const HUNDRED_MILLIS: Duration = Duration::from_millis(100);
    let corro: Sample = SampleSvg::Corro.into();
    let bun: Sample = SamplePng::Bun.into();
    let rgb8: Sample = SampleJpg::Rgb8.into();
    let time = FakeTimeSource::default();

    // Setup server
    let server = image_server();
    let [corro_key, bun_key, rgb8_key] = [corro, bun, rgb8]
        .map(|sample| server.mount_image(RemoteImage::from(sample).cache_control(IMMUTABLE_C_C)));

    // Make a cache where the three images above are just barely too large to all be stored in the
    // cache
    let corro_text = std::str::from_utf8(corro.pre_decode()).unwrap();
    let corro_stable = StableImage::from_svg(corro_text);
    let corro_stored_size = stable_image_data_len(corro_stable);
    let bun_stored_size = stored_image_data_len(bun.post_decode(&Default::default()));
    let rgb8_stored_size = stored_image_data_len(rgb8.post_decode(&Default::default()));
    let just_barely_too_small = corro_stored_size + bun_stored_size + rgb8_stored_size - 1;
    let (_db_dir, mut cache) = cache_builder()
        .time(time.clone())
        .max_size(just_barely_too_small)
        .temp_file();

    cache.from_remote_src(&corro_key).expect("Initial fetch");
    time.inc(HUNDRED_MILLIS);
    cache.from_remote_src(&bun_key).expect("Initial fetch");
    time.inc(HUNDRED_MILLIS);
    cache.from_l1(&corro_key).expect("Still in cache");
    time.inc(HUNDRED_MILLIS);
    cache.from_remote_src(&rgb8_key).expect("Initial fetch");
    // TODO: how to run the garbage collector on the cache?
    // TODO: should add support for garbage collecting the in-memory cache?
    // cache
    //     .from_remote_src(&key)
    //     .expect("Fetch from remote and populate cache");
    // cache.from_l1(&key).expect("L1 of private cache");
    // let mut fresh_l1_cache = cache_builder().open_in(&db_path);
    // fresh_l1_cache.from_l2(&key).expect("L2 of private cache");
}

// Entries that haven't been used in a long time will be evicted based on a global time-to-live
// (TTL)
#[test]
#[ignore = "TODO: waiting for garbage collection"]
fn global_ttl() {
    log::init();

    // TODO: Have three immutable entries and continue using one and ensure that its the one that
    // doesn't get evicted
    todo!();
}

// Parsing of some of the entries could theoretically fail if some corruption went undetected
// somehow (or someone was directly storing invalid data). We should be durable when handling these
// kinds of failures
#[test]
fn corrupt_db_entry() {
    log::init();

    // Setup server
    let server = image_server();
    let sample: Sample = SamplePng::Bun.into();
    let expected_data = sample.post_decode(&Default::default());
    let key = server.mount_image(RemoteImage::from(sample).cache_control(IMMUTABLE_C_C));

    // Populate the cache
    let (_tmp_dir, db_path) = temp::dir();
    let mut cache = cache_builder().open_in(&db_path);
    let data = cache.from_remote_src(&key).unwrap();

    // Ensure we can fetch the cached item
    assert_eq!(data, expected_data, "Bad initial fetch");
    let mut fresh_l1_cache = cache_builder().open_in(&db_path);
    fresh_l1_cache.from_l2(&key).unwrap();

    // Corrupt the cached image
    let conn = rusqlite::Connection::open(cache.path().unwrap()).unwrap();
    conn.execute(
        "update images set image = ?1 where url = ?2",
        ([], key.get()),
    )
    .unwrap();

    // The entry is corrupt, so it re-fetches from source and heals the corrupt entry
    fresh_l1_cache = cache_builder().open_in(&db_path);
    let data = fresh_l1_cache.from_remote_src(&key).unwrap();
    assert_eq!(data, expected_data);
    fresh_l1_cache = cache_builder().open_in(&db_path);
    let data = fresh_l1_cache.from_l2(&key).unwrap();
    assert_eq!(data, expected_data);
}

// Failing to render an image should gracefully return an error
#[test]
fn invalid_img() {
    log::init();

    let truncated_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://"##;
    let (_image_file, image_path) = temp::file_with_suffix(".svg");
    fs::write(&image_path, truncated_svg).unwrap();
    let key = Key::from_abs_path(image_path).expect("Path is internally canonicalized");

    let (_db_dir, mut cache) = cache_builder().temp_file();
    let err = cache
        .err(&key)
        .expect("Can't be decoded as any of our supported image types");
    assert_eq!(err, ImageError::InvalidSvg, "Can't be decoded");
}

// Failing to fetch from a remote server should gracefully return an error
#[test]
fn remote_404_error() {
    log::init();

    // Setup server
    let server = image_server();
    let key = server.mount_image(SamplePng::Bun);

    let (_db_dir, mut cache) = cache_builder().temp_file();
    cache
        .from_remote_src(&key)
        .expect("Can fetch, but won't cache");
    drop(server);
    let err = cache
        .err(&key)
        .expect("Server is shutdown and the image is not cached");
    assert_eq!(err, ImageError::ReqFailed);
}

// We're a private cache, so we can store responses that indicate they're such
#[test]
fn private_cache() {
    log::init();

    // Setup server
    let server = image_server();
    let c_c = IMMUTABLE_C_C.private();
    let key = server.mount_image(RemoteImage::from(SamplePng::Bun).cache_control(c_c));

    let (_tmp_dir, db_path) = temp::dir();
    let mut cache = cache_builder().open_in(&db_path);
    cache
        .from_remote_src(&key)
        .expect("Fetch from remote and populate cache");
    cache.from_l1(&key).expect("L1 of private cache");
    let mut fresh_l1_cache = cache_builder().open_in(&db_path);
    fresh_l1_cache.from_l2(&key).expect("L2 of private cache");
}

// We shouldn't just store everything. A prime candidate to test being the `no-store` directive
#[test]
fn selectively_stores() {
    log::init();

    // Setup server
    let server = image_server();
    let c_c = cache_control().no_store();
    let key = server.mount_image(RemoteImage::from(SamplePng::Bun).cache_control(c_c));

    // Setup cache
    let (_tmp_file, mut cache) = cache_builder().temp_file();

    // Because the image is `no-store` it will fetch from source each time
    cache.from_remote_src(&key).expect("Empty cache");
    cache.from_remote_src(&key).expect("Still empty");
    assert_eq!(
        num_l2_entries(&cache.path().unwrap()),
        0,
        "Shouldn't be in the cache at all"
    );
}

#[test]
#[ignore = "TODO: waiting for garbage collection"]
fn mutli_client_mash() {
    log::init();

    // TODO: This test is a stress-test verifying various assertions while having multiple
    // simulated clients simultaneously blasting away at the same global cache. The test goes
    // roughly as follows:
    //
    // - Setup a fake set of _N_ images that each have a timeline for when they change over time
    // - Have this set of images provided by a test file server under our control (this information
    //   represents our central source of truth)
    // - Spawn _M_ clients (at least one) which will all be moved into separate threads to simulate
    //   multiple clients simultaneously using the cache. One of the threads (the main thread) will
    //   get some extra responsibilities
    // - Each client will get a plan consisting of multiple request sessions of sets of images
    //   using a client-local layered cache. Along with disconnects and reconnects to reset the
    //   in-memory cache
    // - The clients are all synchronized at specific checkpoints coordinated by the main thread
    // - The main thread is also in charge of updating the fake time source since updating that
    //   from multiple threads would muddle things really easily. The way that this is driven is
    //   through extra synchronization points between image request sets and the server responding
    //   to provide specific areas where we can safely update the global time
    //
    // This whole setup allows up to verify some fun properties around the caching. Namely:
    //
    // - The requests to the cache may serve stale content (as long as it's still fresh according
    //   to its cache policy), but it should always match the forward progress of the image changes
    //   decided as the source of truth
    //
    // The source of truth can store all of the relevant info needed to verify the above and it's
    // shared by all of the clients and the image server
    todo!();
}
