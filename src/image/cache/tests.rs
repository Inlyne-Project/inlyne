// TODO: test:
//
// - E-tag match and miss
// - That LRU time appears to get updated right
// - Local image cache appearing to work right
// - That there's correct isolation even when a bunch of simultaneous sessions are hammering the
//   cache including garbage collection
// - Iterate over all the images then snapshot what the stats look like

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime},
};

use super::{L1Check, LayeredCache, LayeredCacheWorker, RemoteKey, SvgContext, TimeSource};
use crate::{
    image::{cache::ImageError},
    test_utils::{log, image, server, temp},
};

use parking_lot::RwLock;

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
    path: &'static str,
    content_type: server::ContentType,
    body: Vec<u8>,
}

impl RemoteImage {
    fn from_sample(cache_control: server::CacheControl, path: &'static str, sample: image::Sample) -> Self {
        Self::new(cache_control, path, sample.into(), sample.pre_decode())
    }

    fn new(
        cache_control: server::CacheControl,
        path: &'static str,
        content_type: server::ContentType,
        body: Vec<u8>,
    ) -> Self {
        dbg!(&cache_control);
        Self {
            cache_control,
            path,
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

const DUMMY_SVG_CTX: SvgContext = SvgContext { dpi: 1.0 };

fn file_cache(time: FakeTimeSource, path: &Path) -> LayeredCacheWorker {
    LayeredCache::new_with_time(time, DUMMY_SVG_CTX).unwrap().from_file(path).unwrap()
}

// TODO: add another option that has an in-memory global db
fn in_memory_cache(time: FakeTimeSource) -> LayeredCacheWorker {
    LayeredCache::new_with_time(time, DUMMY_SVG_CTX).unwrap().in_memory()
}

fn image_server(images: Vec<RemoteImage>) -> server::MiniServerHandle {
    let files = images.into_iter().map(Into::into).collect();
    server::mock_file_server(files)
}

const ONE_HOUR: Duration = Duration::from_secs(60 * 60);
const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);

// TODO: helper function that fetches the image and returns an enum indicating it's source

// Ensures that we can fetch a remote image from each layer of the cache
#[test]
fn layers() {
    log::init();

    // Setup server
    let image: image::Sample = image::SamplePng::Bun.into();
    let expected_data = image.post_decode();
    let cache_control = server::CacheControl::new().immutable();
    let remote_image = RemoteImage::from_sample(cache_control, "/sample.png", image);
    let server = image_server(vec![remote_image]);
    let url = server.url().to_owned() + "/sample.png";
    let image_key = RemoteKey::new_unchecked(url);

    // Setup cache
    let shared_time = FakeTimeSource::default();
    let (_tmp_dir, tmp_path) = temp::dir();
    let db_path = tmp_path.join("test.db");
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

#[test]
fn local_image() {
    todo!();
    // let key = Key::from_abs_path(path)
}

// TODO: add a cache builder that can configure various sizes along with allowing storing locally
// hosted files. something like `.allow_local_urls().cache_size_limit(...).entry_size_limit(...)`

#[test]
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
    // todo!();
}
