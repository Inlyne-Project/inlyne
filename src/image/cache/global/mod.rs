use std::{fmt, fs, path::PathBuf, time::SystemTime};

use super::{Key, RemoteKey, StandardRequest, StoredImage};
use crate::{
    image::ImageData,
    utils::{self, inlyne_cache_dir},
};

use anyhow::Context;
use http::request;
use http_cache_semantics::{BeforeRequest, CachePolicy, RequestLike};
use redb::{AccessGuard, Database, TableDefinition};

mod redb_impls;

// TODO: corrupt DB can panic. Should we switch to just storing blobs of bytes and handle fallible
// parsing of the data externally? If the data failed to parse then that indicates the DB is
// corrupt and should be totally reset per:
// https://github.com/cberner/redb/issues/802#issuecomment-2093364141
// TODO: store a counter to act as a generation, so that we can keep track of the consistency of
// the value between txns
// Access to metadata should be fast, so we keep it in a separate table to avoid loading bulky
// image data except when necessary
const META: TableDefinition<RemoteKey, RemoteMeta> = TableDefinition::new("remote-meta");
const DATA: TableDefinition<RemoteKey, StoredImage> = TableDefinition::new("image-data");

// The database is currently externally versioned meaning that we switch to an entirely new file
// when we bump the version
// TODO: Garbage collection should also be adjusted to cleanup unused databases over time
const VERSION: u32 = 0;

fn db_name() -> String {
    format!("image-cache-v{VERSION}.redb")
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

#[derive(Debug)]
pub struct RemoteMeta {
    pub last_used: SystemTime,
    pub policy: CachePolicy,
}

pub fn run_garbage_collector() -> anyhow::Result<()> {
    let cache = Cache::load()?;
    cache.run_garbage_collector()
}

pub struct Cache(Database);

impl Cache {
    pub fn load() -> anyhow::Result<Self> {
        let cache_dir = inlyne_cache_dir().context("Unable to locate cache dir")?;
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache dir at: {}", cache_dir.display()))?;
        let db_path = db_path()?;
        let file = fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(&db_path)
            .with_context(|| format!("Failed to create database at: {}", db_path.display()))?;
        Self::load_from_file(file)
    }

    pub fn load_from_file(file: fs::File) -> anyhow::Result<Self> {
        let db = Database::builder()
            .create_file(file)
            .context("Failed creating database")?;
        Ok(Self(db))
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        use redb::backends::InMemoryBackend;
        let backend = InMemoryBackend::new();
        let db = Database::builder()
            .create_with_backend(backend)
            .expect("In-memory backend should be infallible");
        Self(db)
    }

    pub fn check_remote_cache(&self, key: &RemoteKey) -> anyhow::Result<CacheCheck> {
        let check = self.check_remote_cache_inner(key)?.unwrap_or_else(|| {
            let req: StandardRequest = key.into();
            let parts = (&req).into();
            CacheCont::Miss(parts).into()
        });
        Ok(check)
    }

    pub fn check_remote_cache_inner(&self, key: &RemoteKey) -> anyhow::Result<Option<CacheCheck>> {
        // TODO: avoid re-doing this
        let req: StandardRequest = key.into();
        let read_txn = self.0.begin_read()?;
        let meta_table = read_txn.open_table(META)?;
        let maybe_check = match meta_table.get(key)?.map(|e| e.value()) {
            None => None,
            Some(meta) => match meta.policy.before_request(&req, SystemTime::now()) {
                BeforeRequest::Fresh(_) => {
                    let data_table = read_txn.open_table(DATA)?;
                    match data_table.get(key)? {
                        Some(entry) => {
                            let data = entry.value();
                            // NOTE: both readers _and_ a single writer can exist simultaneous, so
                            // it's fine to start a write txn even though we already have a read
                            // txn open
                            let write_txn = self.0.begin_write()?;
                            todo!("Update the last used time");
                            Some(CacheCheck::Fresh(data.into()))
                        }
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
                        let data_table = read_txn.open_table(DATA)?;
                        data_table.get(key)?.map(|e| {
                            let data = e.value();
                            CacheCont::TryRefresh((request, data)).into()
                        })
                    }
                }
            },
        };

        Ok(maybe_check)
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
    Fresh(StoredImage),
    Cont(CacheCont),
}

impl From<ImageData> for CacheCheck {
    fn from(data: ImageData) -> Self {
        Self::Fresh(data.into())
    }
}

impl From<CacheCont> for CacheCheck {
    fn from(cont: CacheCont) -> Self {
        Self::Cont(cont)
    }
}

#[must_use]
pub enum CacheCont {
    TryRefresh((request::Parts, StoredImage)),
    Miss(request::Parts),
}
