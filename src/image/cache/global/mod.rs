use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::{RemoteKey, StableImage, StandardRequest};
use crate::{image::cache::global, utils};

use anyhow::Context;
use http::request;
use http_cache_semantics::{BeforeRequest, CachePolicy, RequestLike};
use serde::{Deserialize, Serialize};

mod db;
pub mod wrappers;

// The database is currently externally versioned meaning that we switch to an entirely new file
// when we bump the version
// TODO: Garbage collection should also be adjusted to cleanup unused databases over time
const VERSION: u32 = 0;

pub fn db_name() -> String {
    format!("image-cache-v{VERSION}.db3")
}

fn db_path() -> anyhow::Result<PathBuf> {
    let cache_dir = utils::inlyne_cache_dir().context("Failed to locate cache dir")?;
    let db_path = cache_dir.join(db_name());
    Ok(db_path)
}

pub struct Stats {
    pub path: PathBuf,
    pub inner: Option<StatsInner>,
}

pub struct StatsInner {
    pub size: Bytes,
}

impl Stats {
    pub fn detect() -> anyhow::Result<Stats> {
        let path = db_path()?;
        path.try_into()
    }
}

impl TryFrom<PathBuf> for Stats {
    type Error = anyhow::Error;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        let inner = if !path.is_file() {
            None
        } else {
            let meta = fs::metadata(&path)?;
            let size = meta.len().into();
            let inner = StatsInner { size };
            Some(inner)
        };

        Ok(Self { path, inner })
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { path, inner } = self;
        match inner {
            None => write!(f, "path (not found): {}", path.display()),
            Some(inner) => {
                writeln!(f, "path: {}", path.display())?;
                write!(f, "total size: {}", inner.size)
            }
        }
    }
}

pub struct Bytes(u64);

impl From<u64> for Bytes {
    fn from(bytes: u64) -> Self {
        Self(bytes)
    }
}

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut unit = "B";
        let mut dividend = 1;
        while self.0 / dividend / 1_024 > 1 {
            unit = match unit {
                "B" => "KiB",
                "KiB" => "MiB",
                _ => break,
            };
            dividend *= 1_024;
        }

        write!(f, "{} {}", self.0 / dividend, unit)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoteMeta {
    // TODO: switch to a content hash or uuid v4
    /// A generation used to uniquely identify this cache entry
    ///
    /// We use generations  to keep track of the consistency of a cache entry between different
    /// tranactions. If we increment the generation every time we invalidate the entry in some way
    /// (e.g. changing the stored image) then we're able to keep track of if we're still referring
    /// to the same image in siturations like iniital validation/revalidation/etc.
    pub generation: u32,
    pub last_used: SystemTime,
    pub policy: CachePolicy,
}

pub fn run_garbage_collector() -> anyhow::Result<()> {
    let cache = Cache::load()?;
    cache.run_garbage_collector()
}

pub struct Cache(db::Db);

impl Cache {
    pub fn load() -> anyhow::Result<Self> {
        let db_path = db::Db::default_path()?;
        Self::load_from_file(&db_path)
    }

    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let db = db::Db::open_or_create(path)?;
        Ok(Self(db))
    }

    // TODO: rename to remove `remote_` since it's always remote now
    pub fn check_remote_cache(
        &self,
        key: &RemoteKey,
        now: SystemTime,
    ) -> anyhow::Result<CacheCheck> {
        let check = self.check_remote_cache_inner(key, now)?.unwrap_or_else(|| {
            let req: StandardRequest = key.into();
            let parts = (&req).into();
            CacheCont::Miss(parts).into()
        });
        Ok(check)
    }

    // TODO: rename to remove `remote_` since it's always remote now
    fn check_remote_cache_inner(
        &self,
        key: &RemoteKey,
        now: SystemTime,
    ) -> anyhow::Result<Option<CacheCheck>> {
        fn is_corrupt_entry(err: &rusqlite::Error) -> bool {
            use rusqlite::Error as E;

            match err {
                E::FromSqlConversionFailure(_, _, conv_err) => {
                    conv_err.is::<global::wrappers::ConvertError>()
                }
                E::IntegralValueOutOfRange(_, _) => true,
                _ => false,
            }
        }

        let meta = match self.0.get_meta(key) {
            Ok(Some(meta)) => meta,
            Ok(None) => return Ok(None),
            Err(err) if is_corrupt_entry(&err) => {
                tracing::warn!(%key, %err, "Ignoring corrupt cache entry");
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        };
        let req: StandardRequest = key.into();

        let maybe_meta = match meta.policy.before_request(&req, now) {
            BeforeRequest::Fresh(_) => {
                let gen = meta.generation;
                match self.0.get_data(key, gen) {
                    Ok(Some(image)) => {
                        self.0.refresh_last_used(key, gen, now)?;
                        Some(CacheCheck::Fresh((meta.policy, image.into())))
                    }
                    Ok(None) => None,
                    Err(err) if is_corrupt_entry(&err) => {
                        tracing::warn!(%key, %err, "Ignoring corrupt cache entry");
                        None
                    }
                    Err(err) => return Err(err.into()),
                }
            }
            BeforeRequest::Stale { request, .. } => {
                // NOTE: We're using comparing the headers of the original and `request`
                // requests as a proxy of `http-cache-semantics` trying to refresh our original
                // data vs just sending the request through unchanged
                if req.headers() == request.headers() {
                    // No change to our usual headers means this is a new request
                    Some(CacheCont::Miss(request).into())
                } else {
                    self.0
                        .get_data(key, meta.generation)?
                        .map(|image| CacheCont::TryRefresh((meta.policy, request, image)).into())
                }
            }
        };

        Ok(maybe_meta)
    }

    pub fn insert(
        &mut self,
        key: &RemoteKey,
        policy: &CachePolicy,
        image: StableImage,
        now: SystemTime,
    ) -> anyhow::Result<()> {
        self.0.insert(key, policy, image, now)
    }

    pub fn run_garbage_collector(&self) -> anyhow::Result<()> {
        // TODO: pass over and remove entries and then run compaction. Can get the size of various
        // parts of the image data table to determine when we should actually run compaction
        // (things generally run better when there are pages that can be reused instead of always
        // compacting down to the minimal size)
        todo!();
    }
}

#[must_use]
pub enum CacheCheck {
    Fresh((CachePolicy, StableImage)),
    Cont(CacheCont),
}

impl From<CacheCont> for CacheCheck {
    fn from(cont: CacheCont) -> Self {
        Self::Cont(cont)
    }
}

#[must_use]
pub enum CacheCont {
    TryRefresh((CachePolicy, request::Parts, StableImage)),
    Miss(request::Parts),
}
