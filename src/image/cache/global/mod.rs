use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::{RemoteKey, StableImage, StandardRequest};
use crate::{
    image::ImageData,
    utils::{self, inlyne_cache_dir},
};

use anyhow::Context;
use http::request;
use http_cache_semantics::{BeforeRequest, CachePolicy, RequestLike};
use serde::{Deserialize, Serialize};

mod db;
mod wrappers;

// The database is currently externally versioned meaning that we switch to an entirely new file
// when we bump the version
// TODO: Garbage collection should also be adjusted to cleanup unused databases over time
const VERSION: u32 = 0;

fn db_name() -> String {
    format!("image-cache-v{VERSION}.db3")
}

fn db_path() -> anyhow::Result<PathBuf> {
    let cache_dir = utils::inlyne_cache_dir().context("Failed to locate cache dir")?;
    let db_path = cache_dir.join(db_name());
    Ok(db_path)
}

pub fn stats() -> anyhow::Result<Stats> {
    Stats::new()
}

pub struct Stats {
    pub path: PathBuf,
    pub inner: Option<StatsInner>,
}

pub struct StatsInner {
    pub size: Bytes,
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

impl Stats {
    fn new() -> anyhow::Result<Stats> {
        let path = db_path()?;

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

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoteMeta {
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
        let db = db::Db::load()?;
        Ok(Self(db))
    }

    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let db = db::Db::load_from_file(path)?;
        Ok(Self(db))
    }

    #[cfg(test)]
    pub fn load_in_memory() -> Self {
        let db = db::Db::load_in_memory().expect("Fresh in-memory DB");
        Self(db)
    }

    // TODO: rename to remove `remote_` since it's always remote now
    pub fn check_remote_cache(&self, key: &RemoteKey) -> anyhow::Result<CacheCheck> {
        let check = self.check_remote_cache_inner(key)?.unwrap_or_else(|| {
            let req: StandardRequest = key.into();
            let parts = (&req).into();
            CacheCont::Miss(parts).into()
        });
        Ok(check)
    }

    // TODO: rename to remove `remote_` since it's always remote now
    pub fn check_remote_cache_inner(&self, key: &RemoteKey) -> anyhow::Result<Option<CacheCheck>> {
        let req: StandardRequest = key.into();
        let maybe_meta = match self.0.get_meta(key)? {
            None => None,
            Some(meta) => match meta.policy.before_request(&req, SystemTime::now()) {
                BeforeRequest::Fresh(_) => {
                    let gen = meta.generation;
                    match self.0.get_data(key, gen)? {
                        Some(image) => {
                            self.0.refresh_last_used(key, gen)?;
                            Some(CacheCheck::Fresh((meta.policy, image.into())))
                         },
                        None => None,
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
                        self.0.get_data(key, meta.generation)?.map(|image| {
                            CacheCont::TryRefresh((request, image)).into()
                        })
                    }
                }
            },
        };

        Ok(maybe_meta)
    }

    pub fn insert(&mut self, key: &RemoteKey, policy: &CachePolicy, image: StableImage) -> anyhow::Result<()> {
        self.0.insert(key, policy, image)
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
    TryRefresh((request::Parts, StableImage)),
    Miss(request::Parts),
}
