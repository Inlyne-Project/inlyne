//! References:
//!
//! - <https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control>
//! - <https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Age>
//! - <https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/ETag>

// TODO(cosmic): can we extract out most of the remote cache control flow into its own generic
// crate?

use std::{
    fmt,
    str::{FromStr, Split},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CacheControlMeta {
    e_tag: Option<ETag>,
    stale_after: SystemTime,
}

impl CacheControlMeta {
    pub fn from_resp(resp: &ureq::Response) -> Option<Self> {
        let headers = resp.into();
        let now = SystemTime::now();
        Self::from_headers_with_time(headers, now)
    }

    fn from_headers_with_time(headers: Headers, time: SystemTime) -> Option<Self> {
        let Headers {
            e_tag,
            age,
            cache_control,
        } = headers;
        let e_tag = e_tag.map(Into::into);
        let age = match age {
            Some(age) => {
                let res = age.parse::<Age>();
                if let Err(err) = &res {
                    tracing::info!("Error parsing `Age`: {err}")
                }
                let age = res.ok()?;
                Some(age)
            }
            None => None,
        };
        let mut max_age = None;
        if let Some(cache_control) = cache_control {
            for directive in CacheControlIter::new(cache_control) {
                match directive {
                    CacheControlDirective::NoStore => return None,
                    CacheControlDirective::MaxAge(age) => max_age = Some(age),
                    CacheControlDirective::Ignored => {}
                }
            }
        }

        max_age.and_then(|Age(mut max_age)| {
            if let Some(age) = age {
                max_age = max_age.checked_sub(age.0)?;
            }
            Some(Self::from_e_tag_stale_after_and_time(e_tag, max_age, time))
        })
    }

    fn from_e_tag_stale_after_and_time(
        e_tag: Option<ETag>,
        stale_after: Duration,
        time: SystemTime,
    ) -> Self {
        let stale_after = time + stale_after;
        Self { e_tag, stale_after }
    }
}

#[derive(Default)]
struct Headers<'resp> {
    e_tag: Option<&'resp str>,
    age: Option<&'resp str>,
    cache_control: Option<&'resp str>,
}

impl<'resp> From<&'resp ureq::Response> for Headers<'resp> {
    fn from(resp: &'resp ureq::Response) -> Self {
        Headers {
            e_tag: resp.header("ETag"),
            age: resp.header("Age"),
            cache_control: resp.header("Cache-Control"),
        }
    }
}

/// Represents the `Age` header
///
/// Can be used in tandem with `Cache-Control` to indicate the current age of the request
struct Age(Duration);

#[derive(Debug)]
struct UnknownAge(String);

impl fmt::Display for UnknownAge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown age: {}", self.0)
    }
}

impl std::error::Error for UnknownAge {}

impl FromStr for Age {
    type Err = UnknownAge;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let age = s.parse().map_err(|_| UnknownAge(s.to_owned()))?;
        Ok(Self(Duration::from_secs(age)))
    }
}

/// Represents the `ETag` header
///
/// There's an optional prefix that indicates whether the tag is weak as opposed to strong. Strong
/// tags identify that the content is exactly identical meaning that it can be re-used for things
/// like byte-range requests (Note: we currently ignore strong vs weak as we only require
/// semantically identical responses)
///
/// E-Tags can be used to improve the caching of responses. responses can send an E-Tag to identify
/// the underlying entity. Requests can then send the value of the E-Tag through the
/// `If-None-Match` header where a `304 Not Modified` status indicates that you can re-serve the
/// resource (and more importantly for us, we can re-fresh the `Cache-Control` info if possible)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ETag(String);

impl<'a> From<&'a str> for ETag {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Represents the `Cache-Control` response header directives
///
/// There's a lot involved with cache-control, but _luckily_ we can ignore most of it. Namely:
///
/// - We're a private cache which is strictly stronger than a shared cache, so we can ignore
///   anything to do with visibility, or that's shared cache specific
///   - `s-maxage`
///   - `private`
///   - `public`
/// - We're going to avoid revalidation since that seems like a ton of complexity, and likely isnt
///   that common in the wild
///   - `no-cache`
///   - `must-revalidate`
///   - `proxy-revalidate`
///   - `stale-while-revalidate`
/// - We don't transform the image before caching
///   - `no-transform`
/// - We don't have the concept of reloading cached data yet, so we can't understand the
///   distinction of immutability
///   - `immutable`
/// - We don't understand the requirements for caching based on status codes (Senders should pair
///   this with `no-store` as fallback behavior which we _do_ understand, so we can safely ignore
///   this)
///   - `must-understand`
/// - We're currently ignoring the ability to serve stale requests conditionally on error (would
///   add a lot of complexity)
///   - `stale-if-error`
///
/// That only leaves `max-age` and `no-store`
///
/// - `max-age` indicates that the response can be re-used until after it reaches the given age
///   - Note: The age isn't necessarily 0 as a response can set a current age with the `Age` header
/// - `no-store` indicates that we shouldn't cache the response
enum CacheControlDirective {
    MaxAge(Age),
    NoStore,
    // Used to indicate that we understand a directive exists, but don't follow it
    Ignored,
}

struct CacheControlIter<'header>(Split<'header, char>);

impl<'header> CacheControlIter<'header> {
    fn new(s: &'header str) -> Self {
        Self(s.split(','))
    }
}

impl Iterator for CacheControlIter<'_> {
    type Item = CacheControlDirective;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.0.next()?.parse() {
                Ok(dir) => break Some(dir),
                Err(err) => tracing::info!("Error parsing `Cache-Control` directive: {err:?}"),
            }
        }
    }
}

#[derive(Debug)]
enum CacheControlParseError {
    UnknownDirective(String),
    UnknownAge(UnknownAge),
}

impl fmt::Display for CacheControlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownDirective(unknown) => write!(f, "Unknown directive: {unknown}"),
            Self::UnknownAge(age) => write!(f, "{age}"),
        }
    }
}

impl std::error::Error for CacheControlParseError {}

impl FromStr for CacheControlDirective {
    type Err = CacheControlParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let (directive, arg) = match trimmed.split_once('=') {
            Some((d, arg)) => (d, Some(arg)),
            None => (trimmed, None),
        };
        // Directive is case insensitive
        let norm_directive = directive.to_lowercase();
        match (norm_directive.as_str(), arg) {
            ("max-age", Some(age)) => {
                let age = age.parse().map_err(CacheControlParseError::UnknownAge)?;
                Ok(Self::MaxAge(age))
            }
            ("no-store", None) => Ok(Self::NoStore),
            ("stale-while-revalidate", Some(_)) => Ok(Self::Ignored),
            (
                "s-max-age" | "no-cache" | "must-revalidate" | "proxy-revalidate" | "private"
                | "public" | "must-understand" | "no-transform" | "immutable" | "stale-if-error",
                None,
            ) => Ok(Self::Ignored),
            _ => Err(CacheControlParseError::UnknownDirective(s.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! c {
        ($test_name:ident, $headers:expr, expect: None) => {
            c!($test_name, $headers, None);
        };
        ($test_name:ident, $headers:expr, expect: (stale_after: $stale_after:expr)) => {
            c!($test_name, $headers, expect: (None, $stale_after));
        };
        ($test_name:ident, $headers:expr, expect: ($e_tag:expr, $stale_after:expr)) => {
            c!($test_name, $headers, Some(CacheControlMeta::from_e_tag_stale_after_and_time(
                $e_tag,
                $stale_after,
                SystemTime::UNIX_EPOCH,
            )));
        };
        ($test_name:ident, $headers:expr, $expect:expr) => {
            #[test]
            fn $test_name() {
                $crate::test_utils::init_test_log();

                let cache_meta = CacheControlMeta::from_headers_with_time(
                    $headers,
                    SystemTime::UNIX_EPOCH,
                );
                assert_eq!($expect, cache_meta);
            }
        };
    }

    fn h() -> Headers<'static> {
        Headers::default()
    }

    impl Headers<'static> {
        fn e_tag(mut self, e_tag: &'static str) -> Self {
            self.e_tag = Some(e_tag);
            self
        }

        fn age(mut self, age: &'static str) -> Self {
            self.age = Some(age);
            self
        }

        fn cache(mut self, c_c: &'static str) -> Self {
            self.cache_control = Some(c_c);
            self
        }
    }

    static E_TAG: &str = r#"W/"f855e1c49b6a108e1f4f8ac2e759be3275830224c21ac88a5c15bf8a2e6ee30d""#;
    static ONE_WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
    static ONE_WEEK_C_C: &str = "max-age=604800";
    static HUNDRED_SECS: Duration = Duration::from_secs(100);

    // `no-store` means we shouldn't cache the value at all (not to be confused with `no-cache`)
    c!(plain_no_store, h().cache("no-store"), expect: None);
    // > Typically, arguments for the directives are integers and are therefore not enclosed in
    // > quote characters (e.g., `Cache-control: max-age=12`).
    c!(invalid_age, h().age("\"1\"").cache(ONE_WEEK_C_C), expect: None);
    c!(invalid_max_age, h().cache("max-age=\"100\""), expect: None);
    // > Note that `max-age` is not the elapsed time since the response was received; it is the
    // > elapsed time since the response was generated on the origin server. So if the other
    // > cache(s) -- on the network route taken by the response -- store the response for 100
    // > seconds (indicated using the `Age` response header field), the browser cache would deduct
    // > 100 seconds from its freshness lifetime.
    //
    // In this case the entry would no longer be fresh
    c!(age_past_life, h().age("6").cache("max-age=5"), expect: None);
    // > `must-understand` should be coupled with no-store for fallback behavior.
    //
    // ^^ is expected from the response sender, so not up to us to uphold
    c!(sane_must_understand, h().cache("must-understand, no-store"), expect: None);
    // Sanity check that we can handle unknown directives gracefully
    c!(unknown_directive, h().cache("invalid-directive=100"), expect: None);

    c!(sanity, h().cache(ONE_WEEK_C_C), expect: (stale_after: ONE_WEEK));
    // > - Caching directives are case-insensitive.
    c!(directive_case_insensitive, h().cache("MaX-aGe=604800"), expect: (stale_after: ONE_WEEK));
    c!(e_tag, h().cache("max-age=100").e_tag(E_TAG), expect: (Some(E_TAG.into()), HUNDRED_SECS));
    // Like `age_past_life`, but we're still fresh
    c!(
        age_and_max_age,
        h().cache(ONE_WEEK_C_C).age("100"),
        expect: (stale_after: ONE_WEEK - HUNDRED_SECS)
    );
    c!(
        several_ignored_directives,
        h().cache("max-age=100, must-revalidate, private"),
        expect: (stale_after: HUNDRED_SECS)
    );
}
