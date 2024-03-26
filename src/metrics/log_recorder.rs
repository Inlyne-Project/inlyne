use std::sync::Arc;

use super::{counter, gauge, hist, Unit};

use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, SharedString};
use metrics_util::registry::{Registry, Storage};

struct MetricStore;

impl Storage<Key> for MetricStore {
    type Counter = Arc<counter::Handle>;
    type Gauge = Arc<gauge::Handle>;
    type Histogram = Arc<hist::Handle>;

    fn counter(&self, key: &Key) -> Self::Counter {
        Arc::new(counter::Handle::new(key.to_owned(), None))
    }

    fn gauge(&self, key: &Key) -> Self::Gauge {
        Arc::new(gauge::Handle::new(key.to_owned(), None))
    }

    fn histogram(&self, key: &Key) -> Self::Histogram {
        Arc::new(hist::Handle::new(key.to_owned(), None))
    }
}

pub struct LogRecorder(Registry<Key, MetricStore>);

impl Default for LogRecorder {
    fn default() -> Self {
        Self(Registry::new(MetricStore))
    }
}

impl metrics::Recorder for LogRecorder {
    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, _desc: SharedString) {
        let key = Key::from_name(key);
        let gauge = self.0.get_or_create_histogram(&key, Arc::clone);
        gauge.0.lock().unit = unit.unwrap_or(Unit::Count);
    }

    fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
        let gauge = self.0.get_or_create_gauge(key, Arc::clone);
        Gauge::from_arc(gauge)
    }

    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, _desc: SharedString) {
        let key = Key::from_name(key);
        let counter = self.0.get_or_create_histogram(&key, Arc::clone);
        counter.0.lock().unit = unit.unwrap_or(Unit::Count);
    }

    fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
        let counter = self.0.get_or_create_counter(key, Arc::clone);
        Counter::from_arc(counter)
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, _desc: SharedString) {
        let key = Key::from_name(key);
        let hist = self.0.get_or_create_histogram(&key, Arc::clone);
        hist.0.lock().unit = unit.unwrap_or(Unit::Count);
    }

    fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
        let hist = self.0.get_or_create_histogram(key, Arc::clone);
        Histogram::from_arc(hist)
    }
}
