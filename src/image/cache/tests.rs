use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use super::{Key, L1Check, LayeredCache, LayeredCacheWorker, RemoteKey, SvgContext, TimeSource};
use crate::{
    image::{cache::ImageError, ImageData},
    test_utils::{
        image::{Sample, SampleGif, SampleJpg, SamplePng, SampleSvg, SampleWebp},
        log, server, temp,
    },
};

use parking_lot::RwLock;

fn touch(file: &Path) {
    let now = filetime::FileTime::now();
    filetime::set_file_mtime(file, now).unwrap();
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

#[derive(Clone)]
struct RemoteImage {
    cache_control: server::CacheControl,
    path: String,
    content_type: server::ContentType,
    body: Vec<u8>,
}

impl RemoteImage {
    fn from_sample(
        cache_control: server::CacheControl,
        path: &str,
        sample: Sample,
    ) -> Self {
        Self::new(
            cache_control,
            path,
            sample.into(),
            sample.pre_decode().into(),
        )
    }

    fn new(
        cache_control: server::CacheControl,
        path: &str,
        content_type: server::ContentType,
        body: Vec<u8>,
    ) -> Self {
        Self {
            cache_control,
            path: path.to_owned(),
            content_type,
            body,
        }
    }
}

impl From<RemoteImage> for server::File {
    fn from(image: RemoteImage) -> Self {
        let RemoteImage {
            cache_control,
            path,
            content_type,
            body,
        } = image;
        Self {
            url_path: path.into(),
            mime: content_type,
            cache_control: Some(cache_control),
            bytes: body,
        }
    }
}

fn file_cache(time: FakeTimeSource, path: &Path) -> TestCache {
    let cache = LayeredCache::new_with_time(time, Default::default()).unwrap();
    let src = WorkerSrc::File(path.to_owned());
    TestCache { cache, src }
}

// TODO: add another option that has an in-memory global db
fn in_memory_cache(time: FakeTimeSource) -> TestCache {
    let cache = LayeredCache::new_with_time(time, Default::default()).unwrap();
    let src = WorkerSrc::InMemory;
    TestCache { cache, src }
}

fn image_server(images: Vec<RemoteImage>) -> server::MiniServerHandle {
    let files = images.into_iter().map(Into::into).collect();
    server::mock_file_server(files)
}

const IMMUTABLE_C_C: server::CacheControl = server::CacheControl::new().immutable();
const ONE_HOUR: Duration = Duration::from_secs(60 * 60);
const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);

struct TestCache {
    cache: LayeredCache,
    src: WorkerSrc,
}

enum WorkerSrc {
    File(PathBuf),
    InMemory,
}

impl TestCache {
    fn l1<K: Into<Key>>(&mut self, key: K) -> Option<ImageData> {
        match self.fetch(key) {
            Fetch::L1(data) => Some(data),
            _ => None,
        }
    }

    fn l2_or_src<K: Into<Key>>(&mut self, key: K) -> Option<ImageData> {
        match self.fetch(key) {
            Fetch::L2OrSrc(data) => Some(data),
            _ => None,
        }
    }

    fn err<K: Into<Key>>(&mut self, key: K) -> Option<ImageError> {
        match self.fetch(key) {
            Fetch::Err(e) => Some(e),
            _ => None,
        }
    }

    fn fetch<K: Into<Key>>(&mut self, key: K) -> Fetch {
        let worker = match &self.src {
            WorkerSrc::File(path) => self.cache.from_file(path).unwrap(),
            WorkerSrc::InMemory => self.cache.in_memory(),
        };
        match worker.fetch(key.into()).unwrap() {
            L1Check::Fini(data) => Fetch::L1(data),
            L1Check::Cont(cont) => match cont.finish().unwrap() {
                Ok((_, data)) => Fetch::L2OrSrc(data),
                Err(err) => Fetch::Err(err),
            },
        }
    }
}

enum Fetch {
    L1(ImageData),
    L2OrSrc(ImageData),
    Err(ImageError),
}

/// Consumes a server and ensures that it's fully shutdown
fn ensure_server_shutdown(server: server::MiniServerHandle, key: &Key) {
    let time = FakeTimeSource::default();
    in_memory_cache(time.clone()).l2_or_src(key).expect("Server should be reachable");
    drop(server);
    let err = in_memory_cache(time.clone()).err(key).expect("Empty cache and no server");
    assert_eq!(err, ImageError::ReqFailed, "Server should be shut down");
}

// Ensures that we can fetch a remote image from each layer of the cache. Remote images are stored
// in all layers of the cache
#[test]
fn remote_layers() {
    log::init();

    // Setup server
    let image: Sample = SamplePng::Bun.into();
    let expected_data = image.post_decode(&Default::default());
    let remote_path = format!("/sample{}", image.suffix());
    let remote_image = RemoteImage::from_sample(IMMUTABLE_C_C, &remote_path, image);
    let server = image_server(vec![remote_image]);
    let url = server.url().to_owned() + &remote_path;
    let key = RemoteKey::new_unchecked(url).into();

    // Setup cache
    let shared_time = FakeTimeSource::default();
    let (_tmp_dir, tmp_path) = temp::dir();
    let db_path = tmp_path.join("test.db");
    let mut cache = file_cache(shared_time.clone(), &db_path);

    // Fetch from remote and populate all of the cache layers in the process
    let data = cache.l2_or_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");

    // Shutdown the server
    ensure_server_shutdown(server, &key);

    // Fetch from l1
    let data = cache.l1(&key).expect("L1 is populated");
    assert_eq!(data, expected_data, "Invalid L1 image");

    // Fetch from l2
    // The server is already shutdown and l1 is new, so this can only be from l2
    let mut empty_l1_cache = file_cache(shared_time, &db_path);
    let data = empty_l1_cache.l2_or_src(&key).expect("L2 is populated");
    assert_eq!(data, expected_data, "Invalid L2 image");
}

// Local images are only stored in the in-memory cache since the global cache is exclusively for
// remote images
#[test]
fn local_layers() {
    log::init();

    // Setup local image
    let image: Sample = SampleGif::AtuinDemo.into();
    let (mut image_file, image_path) = temp::file_with_suffix(image.suffix());
    let image_bytes = image.pre_decode();
    io::copy(&mut io::Cursor::new(image_bytes), &mut image_file).unwrap();
    let key = Key::from_abs_path(image_path).expect("Path is internally canonicalized");
    let expected_data = image.post_decode(&Default::default());

    // Setup cache
    let time = FakeTimeSource::default();
    let mut cache = in_memory_cache(time);

    // Fetch from source
    let data = cache.l2_or_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");

    // And now from l1
    let data = cache.l1(&key).expect("L1 is populated");
    assert_eq!(data, expected_data, "Invalid L1 image");
}

// SVGs are a special case since they're stored pre-rendered since the rendering can change session
// to session based on the current dpi and available fonts
//
// Remote SVGs are still stored in all layers of the cache
// TODO(cosmic): rendering can change within a session too by doing things like zooming. We should
// really allow for rerendering within a session too
#[test]
fn remote_fetch_svg() {
    log::init();

    // Setup server
    let image: Sample = SampleSvg::Cargo.into();
    let expected_data = image.post_decode(&Default::default());
    let remote_path = format!("/sample{}", image.suffix());
    let remote_image = RemoteImage::from_sample(IMMUTABLE_C_C, &remote_path, image);
    let server = image_server(vec![remote_image]);
    let url = server.url().to_owned() + &remote_path;
    let key = RemoteKey::new_unchecked(url).into();

    // Setup cache
    let shared_time = FakeTimeSource::default();
    let (_tmp_dir, tmp_path) = temp::dir();
    let db_path = tmp_path.join("test.db");
    let mut cache = file_cache(shared_time.clone(), &db_path);

    // Fetch from remote and populate all of the cache layers in the process
    let data = cache.l2_or_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");

    // Shutdown the server
    ensure_server_shutdown(server, &key);

    // Fetch from l2
    // The server is already shutdown and l1 is new, so this can only be from l2
    let mut empty_l1_cache = file_cache(shared_time.clone(), &db_path);
    let data = empty_l1_cache.l2_or_src(&key).expect("L2 is populated");
    assert_eq!(data, expected_data, "Invalid L2 image");

    // Try fetching again with different DPI and make sure the rendering changes
    let hidpi_ctx = SvgContext { dpi: 2.0 };
    let hidpi_cache = LayeredCache::new_with_time(shared_time, hidpi_ctx.clone()).unwrap();
    let src = WorkerSrc::File(db_path);
    let mut hidpi_cache = TestCache { cache: hidpi_cache, src };
    let hidpi_data = hidpi_cache.l2_or_src(&key).expect("L2 is populated");
    let hidpi_expected = image.post_decode(&hidpi_ctx);
    assert_eq!(hidpi_data, hidpi_expected, "Bad higher dpi fetch");
    assert_ne!(hidpi_data, data, "Rendering changes with different dpi");
}

// Same as remote SVGs, but not stored in L2 like other local images
#[test]
fn local_fetch_svg() {
    log::init();

    // Setup local image
    let image: Sample = SampleSvg::Corro.into();
    let (mut image_file, image_path) = temp::file_with_suffix(image.suffix());
    let image_bytes = image.pre_decode();
    io::copy(&mut io::Cursor::new(image_bytes), &mut image_file).unwrap();
    let key = Key::from_abs_path(image_path).expect("Path is internally canonicalized");
    let expected_data = image.post_decode(&Default::default());

    // Setup cache
    let time = FakeTimeSource::default();
    let mut cache = in_memory_cache(time);

    // Fetch from source
    let data = cache.l2_or_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");

    // And now from l1
    let data = cache.l1(&key).expect("L1 is populated");
    assert_eq!(data, expected_data, "Invalid L1 image");
}

// Remote invalidation is handled by both a global TTL and the entries cache policy info
#[test]
#[ignore = "TODO"]
fn remote_invalidation() {
    log::init();

    todo!();
}

// Local invalidation is handled by checking against the file's last modified time
#[test]
fn local_invalidation() {
    log::init();

    // Setup local image
    let image: Sample = SampleJpg::Rgb8.into();
    let (mut image_file, image_path) = temp::file_with_suffix(image.suffix());
    let image_bytes = image.pre_decode();
    io::copy(&mut io::Cursor::new(image_bytes), &mut image_file).unwrap();
    let key = Key::from_abs_path(image_path.clone()).expect("Path is internally canonicalized");
    let expected_data = image.post_decode(&Default::default());

    // Setup cache
    let time = FakeTimeSource::default();
    let mut cache = in_memory_cache(time);

    // Fetch from source
    let data = cache.l2_or_src(&key).expect("Empty cache");
    assert_eq!(data, expected_data, "Bad initial fetch");

    // It's available in l1
    let data = cache.l1(&key).expect("L1 is populated");
    assert_eq!(data, expected_data, "Invalid L1 image");

    let mut refetched = false;
    for _ in 0..10 {
        touch(&image_path);
        if cache.l2_or_src(&key).is_some() {
            refetched = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(refetched, "Touching local image should invalidate and refetch");
}

// Stale cache entries with an ETag can use a flow to re-validate the stale entry without having to
// send it from the remote server again. This involves sending our stored e-tag in a special
// request header and the server can respond saying that it still matches the remote entry and can
// be refreshed
// TODO: need to add support for this flow to the test image server
#[test]
#[ignore = "TODO"]
fn e_tag_refresh() {
    log::init();

    todo!();
}

#[test]
#[ignore = "TODO"]
fn stats() {
    log::init();

    todo!();
}

// When the cache is over capacity entries will be evicted in order of those that were least
// recently used (LRU)
#[test]
#[ignore = "TODO"]
fn lru() {
    log::init();

    // TODO: insert over the capacity in entries and then have a list that indicates when things
    // were used to verify that we evict things in the right order
    todo!();
}

// Entries that haven't been used in a long time will be evicted based on a global time-to-live
// (TTL)
#[test]
#[ignore = "TODO"]
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
#[ignore = "TODO"]
fn corrupt_db_entry() {
    log::init();

    todo!();
}

// TODO: add a cache builder that can configure various sizes along with allowing storing locally
// hosted files. something like `.allow_local_urls().cache_size_limit(...).entry_size_limit(...)`

#[test]
#[ignore = "TODO"]
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
