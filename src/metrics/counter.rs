use super::{Metric, Unit, SPAN_LEVEL};

use metrics::{CounterFn, Key};
use parking_lot::Mutex;
use tracing::{debug, span};

pub struct Handle(pub Mutex<Metric<u64>>);

impl Handle {
    pub fn new(key: Key, unit: Option<Unit>) -> Self {
        Self(Metric::new(key, 0, unit))
    }
}

impl CounterFn for Handle {
    fn absolute(&self, value: u64) {
        let mut counter = self.0.lock();
        counter.value = value;

        let key = counter.key.name();
        let unit = counter.unit.as_canonical_label();
        let span = span!(SPAN_LEVEL, "counter", %key);
        let _enter = span.enter();
        debug!("set to {value}{unit}");
    }

    fn increment(&self, value: u64) {
        let mut counter = self.0.lock();
        counter.value = counter.value.saturating_add(value);

        let key = counter.key.name();
        let unit = counter.unit.as_canonical_label();
        let counter_value = counter.value;
        let span = span!(SPAN_LEVEL, "counter", %key);
        let _enter = span.enter();
        debug!("incremented by {value}{unit} to {counter_value}{unit}",);
    }
}
