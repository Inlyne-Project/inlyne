// TODO: test:
//
// - E-tag match and miss
// - That LRU time appears to get updated right
// - That both cache layers appear to work right (selectively wipe the cache layers and sources to
//   check)
// - Local image cache appearing to work right
// - That there's correct isolation even when a bunch of simultaneous sessions are hammering the
//   cache including garbage collection

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use http::{HeaderMap, HeaderValue};
use parking_lot::RwLock;

use crate::image::ImageData;

use super::TimeSource;

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

struct RemoteImage {
    headers: Headers,
    body: Vec<u8>,
}

impl RemoteImage {
    fn new(headers: Headers, body: Vec<u8>) -> Self {
        Self { headers, body }
    }
}

#[derive(Default)]
struct Headers {
    max_age: Option<Duration>,
}

impl Headers {
    fn new() -> Self {
        Self::default()
    }

    fn max_age(mut self, age: Duration) -> Self {
        self.max_age = Some(age);
        self
    }
}

impl From<Headers> for HeaderMap {
    fn from(headers: Headers) -> Self {
        let Headers { max_age } = headers;

        let mut map = HeaderMap::new();

        let mut cache_control = Vec::new();
        if let Some(age) = max_age {
            cache_control.push(format!("max-age={}", age.as_secs()));
        }

        if !cache_control.is_empty() {
            let cc = cache_control.join(",");
            map.insert("Cache-Control", cc.parse().unwrap());
        }

        map
    }
}

#[derive(Clone, Copy)]
enum Sample {
    Img1,
    Img2,
    Img3,
}

impl Sample {
    fn pre_decode(self) -> Vec<u8> {
        // TODO: swap these out for b64 encoded strings?
        match self {
            Self::Img1 => include_bytes!("../../../assets/test_data/bun_logo.png").as_slice(),
            Self::Img2 => include_bytes!("../../../assets/test_data/rgba8.jpg").as_slice(),
            Self::Img3 => include_bytes!("../../../assets/test_data/rgba8.png").as_slice(),
        }
        .into()
    }

    fn post_decode(self) -> ImageData {
        ImageData::load(&self.pre_decode(), true).unwrap()
    }
}

const ONE_HOUR: Duration = Duration::from_secs(60 * 60);
const ONE_DAY: Duration = Duration::from_secs(24 * 60 * 60);

#[test]
fn sanity() {
    let image = Sample::Img1;
    let headers = Headers::new().max_age(ONE_DAY);
    let remote_image = RemoteImage::new(headers, image.pre_decode());
    todo!();
}

// TODO: add a cache builder that can configure various sizes along with allowing storing locally
// hosted files. something like `.allow_local_urls().cache_size_limit(...).entry_size_limit(...)`

#[test]
fn mutli_client_mash() {
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
