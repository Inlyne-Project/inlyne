use std::time::Duration;

use super::{describe_histogram, Metric, Unit, SPAN_LEVEL};

use metrics::{HistogramFn, Key, KeyName};
use metrics_util::Summary;
use parking_lot::Mutex;
use tracing::{debug, info, span, trace};

#[derive(Clone, Copy)]
pub enum Tag {
    ImageDecompress,
    ImageLoad,
    Positioner,
    Redraw,
    Reposition,
}

impl Tag {
    pub fn set_global_description(self) {
        describe_histogram!(self.as_str(), self.unit(), self.desc_text());
    }

    pub fn iter() -> TagIter {
        TagIter(Some(Tag::ImageDecompress))
    }

    fn as_str(self) -> &'static str {
        match self {
            Tag::ImageDecompress => "image.decompress",
            Tag::ImageLoad => "image.load",
            Tag::Positioner => "positioner",
            Tag::Redraw => "redraw",
            Tag::Reposition => "reposition",
        }
    }

    pub fn desc_text(self) -> &'static str {
        match self {
            Self::ImageDecompress => "Decompressing image data to render",
            Self::ImageLoad => "Reading, decoding, and compressing the raw image data",
            Self::Positioner => "Positioning all of the elements",
            Self::Redraw => "A full redraw",
            Self::Reposition => "Repositioning all of the elements in the queue",
        }
    }

    pub fn unit(self) -> Unit {
        match self {
            Self::ImageDecompress
            | Self::ImageLoad
            | Self::Positioner
            | Self::Redraw
            | Self::Reposition => Unit::Seconds,
        }
    }
}

impl From<Tag> for KeyName {
    fn from(tag: Tag) -> Self {
        tag.as_str().into()
    }
}

// TODO(cosmic): we can switch to strum if we start doing this a lot
pub struct TagIter(Option<Tag>);

impl Iterator for TagIter {
    type Item = Tag;

    fn next(&mut self) -> Option<Self::Item> {
        let next = std::mem::take(&mut self.0)?;
        self.0 = match next {
            Tag::ImageDecompress => Some(Tag::ImageLoad),
            Tag::ImageLoad => Some(Tag::Positioner),
            Tag::Positioner => Some(Tag::Redraw),
            Tag::Redraw => Some(Tag::Reposition),
            Tag::Reposition => None,
        };
        Some(next)
    }
}

pub struct Handle(pub Mutex<Metric<Summary>>);

impl Handle {
    pub fn new(key: Key, unit: Option<Unit>) -> Self {
        let summary = Summary::with_defaults();
        Self(Metric::new(key, summary, unit))
    }
}

impl HistogramFn for Handle {
    fn record(&self, value: f64) {
        let mut hist = self.0.lock();
        hist.value.add(value);

        let p50 = hist.value.quantile(0.5).expect("Has values");
        let p99 = hist.value.quantile(0.99).expect("Has values");
        let p999 = hist.value.quantile(0.999).expect("Has values");
        let key = hist.key.name();
        let span = span!(SPAN_LEVEL, "histogram", %key);
        let _enter = span.enter();
        // `Duration`s automatically get consumed as seconds by `IntoF64`, so special case
        // `Unit::Seconds` for durations specifically
        if hist.unit == Unit::Seconds {
            let value = Duration::from_secs_f64(value);
            let p50 = Duration::from_secs_f64(p50);
            let p99 = Duration::from_secs_f64(p99);
            let p999 = Duration::from_secs_f64(p999);
            let msg =
                format!("record {value:.02?} | p50 {p50:.02?} | p99 {p99:.02?} | p999 {p999:.02?}");
            if value < p50 {
                trace!("{msg}");
            } else if value < p99 {
                debug!("{msg}");
            } else {
                info!("{msg}");
            }
        } else {
            let unit = hist.unit.as_canonical_label();
            debug!(
                "record {value:.02} | p50 {p50:.02}{unit} | p99 {p99:.02}{unit} | \
                p999 {p999:.02}{unit}"
            );
        }
    }
}
