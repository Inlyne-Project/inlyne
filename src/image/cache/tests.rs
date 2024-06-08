// TODO: test:
//
// - Local image
// - E-tag match and miss
// - That LRU time appears to get updated right
// - Local image cache appearing to work right
// - That there's correct isolation even when a bunch of simultaneous sessions are hammering the
//   cache including garbage collection
// - Iterate over all the images then snapshot what the stats look like

use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime},
};

use super::{L1Check, LayeredCache, LayeredCacheWorker, RemoteKey, SvgContext, TimeSource};
use crate::{
    image::{cache::ImageError, ImageData},
    test_utils::{self, HttpServer},
};

use http::{header, HeaderMap, HeaderValue};
use parking_lot::RwLock;
use tiny_http::{Header, Method, Response, ResponseBox};

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
    cache_control: CacheControl,
    path: &'static str,
    content_type: ContentType,
    body: Vec<u8>,
}

impl RemoteImage {
    fn from_sample(cache_control: CacheControl, path: &'static str, sample: Sample) -> Self {
        Self::new(cache_control, path, sample.into(), sample.pre_decode())
    }

    fn new(
        cache_control: CacheControl,
        path: &'static str,
        content_type: ContentType,
        body: Vec<u8>,
    ) -> Self {
        Self {
            cache_control,
            path,
            content_type,
            body,
        }
    }
}

impl From<RemoteImage> for ResponseBox {
    fn from(image: RemoteImage) -> Self {
        let RemoteImage {
            cache_control,
            path: _,
            content_type,
            body,
        } = image;
        Response::from_data(body)
            .with_header(cache_control)
            .with_header(content_type)
            .boxed()
    }
}

#[derive(Clone, Copy)]
enum ContentType {
    Gif,
    Jpg,
    Png,
    Qoi,
    Svg,
    Webp,
}

impl ContentType {
    fn to_str(self) -> &'static str {
        match self {
            Self::Gif => "image/gif",
            Self::Jpg => "image/jpeg",
            Self::Png => "image/png",
            Self::Qoi => "image/qoi",
            Self::Svg => "image/svg+xml",
            Self::Webp => "image/webp",
        }
    }
}

impl From<Sample> for ContentType {
    fn from(sample: Sample) -> Self {
        match sample {
            Sample::Gif(_) => Self::Gif,
            Sample::Jpg(_) => Self::Jpg,
            Sample::Png(_) => Self::Png,
            Sample::Qoi(_) => Self::Qoi,
            Sample::Svg(_) => Self::Svg,
            Sample::Webp(_) => Self::Webp,
        }
    }
}

impl From<ContentType> for Header {
    fn from(content_ty: ContentType) -> Self {
        let header_name = header::CONTENT_TYPE.as_str().as_bytes();
        let content_ty = content_ty.to_str().as_bytes();
        Header::from_bytes(header_name, content_ty).unwrap()
    }
}

#[derive(Clone, Default)]
struct CacheControl {
    immutable: bool,
    max_age: Option<Duration>,
}

impl CacheControl {
    fn new() -> Self {
        Self::default()
    }

    fn immutable(mut self) -> Self {
        self.immutable = true;
        self
    }

    fn max_age(mut self, age: Duration) -> Self {
        self.max_age = Some(age);
        self
    }

    fn to_header_value(&self) -> Option<String> {
        let CacheControl { immutable, max_age } = self;
        let mut cache_control = Vec::new();
        if *immutable {
            cache_control.push("immutable".to_owned());
        }
        if let Some(age) = max_age {
            cache_control.push(format!("max-age={}", age.as_secs()));
        }

        if !cache_control.is_empty() {
            let cc = cache_control.join(",");
            cc.parse().ok()
        } else {
            None
        }
    }
}

impl From<CacheControl> for Header {
    fn from(cache_control: CacheControl) -> Self {
        let value = cache_control.to_header_value().unwrap();
        Self::from_bytes(header::CACHE_CONTROL.as_str(), value).unwrap()
    }
}

impl From<CacheControl> for HeaderMap {
    fn from(cache_control: CacheControl) -> Self {
        let mut map = HeaderMap::new();

        if let Some(value) = cache_control.to_header_value() {
            let value = HeaderValue::from_str(&value).unwrap();
            map.insert(header::CACHE_CONTROL, value);
        }

        map
    }
}

#[derive(Clone, Copy)]
enum Sample {
    Gif(SampleGif),
    Jpg(SampleJpg),
    Png(SamplePng),
    Qoi(SampleQoi),
    Svg(SampleSvg),
    Webp(SampleWebp),
}

impl From<SampleGif> for Sample {
    fn from(gif: SampleGif) -> Self {
        Self::Gif(gif)
    }
}

impl From<SampleJpg> for Sample {
    fn from(jpg: SampleJpg) -> Self {
        Self::Jpg(jpg)
    }
}

impl From<SamplePng> for Sample {
    fn from(png: SamplePng) -> Self {
        Self::Png(png)
    }
}

impl From<SampleQoi> for Sample {
    fn from(qoi: SampleQoi) -> Self {
        Self::Qoi(qoi)
    }
}

impl From<SampleSvg> for Sample {
    fn from(svg: SampleSvg) -> Self {
        Self::Svg(svg)
    }
}

impl From<SampleWebp> for Sample {
    fn from(webp: SampleWebp) -> Self {
        Self::Webp(webp)
    }
}

#[derive(Clone, Copy)]
enum SampleGif {
    AtuinDemo,
    Rgb8,
    Rgba8,
}

#[derive(Clone, Copy)]
enum SampleJpg {
    Rgb8,
    Rgb8a,
}

#[derive(Clone, Copy)]
enum SamplePng {
    Ariadne,
    Bun,
    Rgb8,
    Rgba8,
}

#[derive(Clone, Copy)]
enum SampleQoi {
    Rgb8,
    Rgba8,
}

#[derive(Clone, Copy)]
enum SampleSvg {
    Corro,
    Cargo,
    Repology,
}

#[derive(Clone, Copy)]
enum SampleWebp {
    CargoPublicApi,
}

impl Sample {
    fn pre_decode(self) -> Vec<u8> {
        // TODO: swap these out for b64 encoded strings?
        match self {
            // TODO: move these includes to somewhere central?
            Self::Jpg(jpg) => match jpg {
                SampleJpg::Rgb8 => include_bytes!("../../../assets/test_data/rgb8.jpg").as_slice(),
                SampleJpg::Rgb8a => {
                    include_bytes!("../../../assets/test_data/rgba8.jpg").as_slice()
                }
            },
            Self::Gif(gif) => match gif {
                SampleGif::AtuinDemo => {
                    include_bytes!("../../../assets/test_data/atuin_demo.gif").as_slice()
                }
                SampleGif::Rgb8 => todo!(),
                SampleGif::Rgba8 => todo!(),
            },
            Self::Png(png) => match png {
                SamplePng::Ariadne => {
                    include_bytes!("../../../assets/test_data/ariadne_example.png").as_slice()
                }
                SamplePng::Bun => {
                    include_bytes!("../../../assets/test_data/bun_logo.png").as_slice()
                }
                SamplePng::Rgb8 => todo!(),
                SamplePng::Rgba8 => todo!(),
            },
            Self::Qoi(qoi) => match qoi {
                SampleQoi::Rgb8 => todo!(),
                SampleQoi::Rgba8 => todo!(),
            },
            Self::Svg(svg) => match svg {
                SampleSvg::Corro => todo!(),
                SampleSvg::Cargo => todo!(),
                SampleSvg::Repology => todo!(),
            },
            Self::Webp(SampleWebp::CargoPublicApi) => todo!(),
        }
        .into()
    }

    fn post_decode(self) -> ImageData {
        ImageData::load(&self.pre_decode(), true).unwrap()
    }
}

const DUMMY_SVG_CTX: SvgContext = SvgContext { dpi: 1.0 };

fn file_cache(time: FakeTimeSource, path: &Path) -> LayeredCacheWorker {
    LayeredCache::new_with_time(time, DUMMY_SVG_CTX).unwrap().from_file(path).unwrap()
}

// TODO: add another option that has an in-memory global db
fn in_memory_cache(time: FakeTimeSource) -> LayeredCacheWorker {
    LayeredCache::new_with_time(time, DUMMY_SVG_CTX).unwrap().in_memory()
}

fn image_server(images: Vec<RemoteImage>) -> HttpServer {
    let images: Arc<[_]> = images.into();
    HttpServer::spawn(images, |images, req| {
        let not_found = Response::empty(404).boxed();
        if req.method() != &Method::Get {
            return not_found;
        }

        for image in images.iter() {
            if image.path == req.url() {
                return image.to_owned().into();
            }
        }

        not_found
    })
}

const ONE_HOUR: Duration = Duration::from_secs(60 * 60);
const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);

// TODO: helper function that fetches the image and returns an enum indicating it's source

// Ensures that we can fetch a remote image from each layer of the cache
#[test]
fn layers() {
    test_utils::init_test_log();

    // Setup server
    let image: Sample = SamplePng::Bun.into();
    let expected_data = image.post_decode();
    let cache_control = CacheControl::new().immutable();
    let remote_image = RemoteImage::from_sample(cache_control, "/sample.png", image);
    let server = image_server(vec![remote_image]);
    let url = server.url().to_owned() + "/sample.png";
    let image_key = RemoteKey::new_unchecked(url);

    // Setup cache
    let shared_time = FakeTimeSource::default();
    let tmp_dir = tempfile::Builder::new().prefix("inlyne-test-").tempdir().unwrap();
    let db_path = tmp_dir.path().join("test.db");
    let mut cache = file_cache(shared_time.clone(), &db_path);

    // Fetch from remote and populate all the cache layers in the process
    let L1Check::Cont(cont) = cache.fetch(image_key.clone()).unwrap() else {
        panic!("L1 shouldn't be populated on a fresh cache");
    };
    let pair = cont.finish().unwrap().unwrap();
    cache = pair.0;
    let data = pair.1;
    assert_eq!(data, expected_data, "Bad initial fetch");

    // Shutdown the server and ensure that requests now fail
    drop(server);
    let throwaway_cache = in_memory_cache(shared_time.clone());
    let L1Check::Cont(cont) = throwaway_cache.fetch(image_key.clone()).unwrap() else {
        panic!("L1 shouldn't be populated on a fresh cache");
    };
    let err = cont.finish().unwrap().unwrap_err();
    assert_eq!(err, ImageError::ReqFailed, "Server should be shut down");

    // Fetch from l1
    let L1Check::Fini(data) = cache.fetch(image_key.clone()).unwrap() else {
        panic!("L1 should be populated");
    };
    assert_eq!(data, expected_data, "Invalid L1 image");

    // Fetch from l2
    let fresh_l1_cache = file_cache(shared_time.clone(), &db_path);
    let L1Check::Cont(cont) = fresh_l1_cache.fetch(image_key.clone()).unwrap() else {
        panic!("L1 shouldn't be populated on a fresh cache");
    };
    let data = cont.finish().unwrap().expect("L2 is populated").1;
    assert_eq!(data, expected_data, "Invalid L2 image");
}

// TODO: add a cache builder that can configure various sizes along with allowing storing locally
// hosted files. something like `.allow_local_urls().cache_size_limit(...).entry_size_limit(...)`

#[test]
fn mutli_client_mash() {
    test_utils::init_test_log();

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
    // todo!();
}
