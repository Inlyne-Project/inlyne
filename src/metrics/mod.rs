//! All of our wrappers and helpers for the `metrics` crate

use metrics::Key;
use parking_lot::Mutex;
use tracing::Level;

// Re-exports from the actual `metrics` crate
pub use metrics::{describe_histogram, histogram, set_global_recorder, Unit};

mod counter;
mod gauge;
mod hist;
mod log_recorder;

pub use hist::Tag as HistTag;
pub use log_recorder::LogRecorder;

const SPAN_LEVEL: Level = Level::INFO;

struct Metric<T> {
    key: Key,
    unit: Unit,
    value: T,
}

impl<T> Metric<T> {
    fn new(key: Key, value: T, unit: Option<Unit>) -> Mutex<Self> {
        let unit = unit.unwrap_or(Unit::Count);
        Mutex::new(Metric { key, unit, value })
    }
}
