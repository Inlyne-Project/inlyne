// TODO: test:
//
// - E-tag match and miss
// - That LRU time appears to get updated right
// - That both cache layers appear to work right (selectively wipe the cache layers and sources to
//   check)
// - Local image cache appearing to work right
// - That there's correct isolation even when a bunch of simultaneous sessions are hammering the
//   cache including garbage collection

use std::{sync::Arc, time::SystemTime};

use parking_lot::RwLock;

use super::TimeSource;

struct FakeTimeSource(Arc<RwLock<SystemTime>>);

impl TimeSource for FakeTimeSource {
    fn now(&self) -> SystemTime {
        self.0.read().to_owned()
    }
}

impl From<SystemTime> for FakeTimeSource {
    fn from(time: SystemTime) -> Self {
        Self(RwLock::new(time).into())
    }
}
