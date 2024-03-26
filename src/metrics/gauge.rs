use super::{Metric, Unit, SPAN_LEVEL};

use metrics::{GaugeFn, Key};
use parking_lot::Mutex;
use tracing::{debug, span};

pub struct Handle(pub Mutex<Metric<f64>>);

impl Handle {
    pub fn new(key: Key, unit: Option<Unit>) -> Self {
        Self(Metric::new(key, 0.0, unit))
    }
}

impl GaugeFn for Handle {
    fn increment(&self, value: f64) {
        let mut gauge = self.0.lock();
        gauge.value += value;

        let key = gauge.key.name();
        let unit = gauge.unit.as_canonical_label();
        let gauge_value = gauge.value;
        let span = span!(SPAN_LEVEL, "gauge", %key);
        let _enter = span.enter();
        debug!("incremented by {value}{unit} to {gauge_value}{unit}",);
    }

    fn decrement(&self, value: f64) {
        let mut gauge = self.0.lock();
        gauge.value -= value;

        let key = gauge.key.name();
        let unit = gauge.unit.as_canonical_label();
        let gauge_value = gauge.value;
        let span = span!(SPAN_LEVEL, "gauge", %key);
        let _enter = span.enter();
        debug!("decremented by {value}{unit} to {gauge_value}{unit}",);
    }

    fn set(&self, value: f64) {
        let mut gauge = self.0.lock();
        gauge.value = value;

        let key = gauge.key.name();
        let unit = gauge.unit.as_canonical_label();
        let span = span!(SPAN_LEVEL, "gauge", %key);
        let _enter = span.enter();
        debug!("set to {value}{unit}",);
    }
}
