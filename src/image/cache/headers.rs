use std::{
    str::{FromStr, Split},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CacheControlMeta {
    e_tag: Option<ETag>,
    stale_after: SystemTime,
}

impl CacheControlMeta {
    pub fn from_resp(resp: &ureq::Response) -> Option<Self> {
        let e_tag = resp.header("ETag").map(Into::into);
        let age = resp.header("Age").and_then(|age| {
            age.parse::<Age>()
                .inspect_err(|err| tracing::info!("Error parsing `Age`: {err:?}"))
                .ok()
        });
        let mut max_age = None;
        if let Some(cache_control) = resp.header("Cache-Control") {
            for directive in CacheControlIter::new(cache_control) {
                match directive {
                    CacheControlDirective::NoStore => return None,
                    CacheControlDirective::MaxAge(age) => max_age = Some(age),
                    CacheControlDirective::Ignored => {}
                }
            }
        }

        max_age.and_then(|max_age| {
            let mut max_age = max_age.0;
            if let Some(age) = age {
                max_age = max_age.checked_sub(age.0)?;
            }
            let now = SystemTime::now();
            let stale_after = now + max_age;
            Some(Self { stale_after, e_tag })
        })
    }
}

/// Represents the `Age` header
///
/// Can be used in tandem with `Cache-Control` to indicate the current age of the request
struct Age(pub Duration);

#[derive(Debug)]
struct UnknownAge(pub String);

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
/// like byte-range requests
// NOTE: We currently ignore weak/strong as we don't need any of "strong"'s guarantees
#[derive(Clone, Debug, Deserialize, Serialize)]
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
            (
                "s-max-age"
                | "no-cache"
                | "must-revalidate"
                | "proxy-revalidate"
                | "private"
                | "public"
                | "must-understand"
                | "no-transform"
                | "immutable"
                | "stale-while-revalidate"
                | "stale-if-error",
                None,
            ) => Ok(Self::Ignored),
            _ => Err(CacheControlParseError::UnknownDirective(s.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanity() {
        todo!("Split out parsing logic into its own function and test against that");
    }
}
