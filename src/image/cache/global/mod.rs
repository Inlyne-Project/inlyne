use std::{fmt, fs, path::PathBuf, time::SystemTime};

use super::{Key, RemoteKey, StandardRequest};
use crate::{
    image::ImageData,
    utils::{self, inlyne_cache_dir},
};

use anyhow::Context;
use http::request;
use http_cache_semantics::{BeforeRequest, CachePolicy};
use redb::{backends::InMemoryBackend, Database, TableDefinition};

mod redb_impls;

// Access to metadata should be fast, so we keep it in a separate table to avoid loading bulky
// image data except when necessary
const REMOTE_META: TableDefinition<RemoteKey, RemoteMeta> = TableDefinition::new("remote-meta");
const IMAGE_DATA: TableDefinition<RemoteKey, ImageData> = TableDefinition::new("image-data");

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
        let backend = InMemoryBackend::new();
        let db = Database::builder()
            .create_with_backend(backend)
            .expect("In-memory backend should be infallible");
        Self(db)
    }

    pub fn check_remote_cache(&self, key: &RemoteKey) -> anyhow::Result<Option<CacheCheck>> {
        // TODO: avoid re-doing this
        let req: StandardRequest = key.into();
        let maybe_check = match self.fetch_remote_meta(&key)? {
            None => None,
            // TODO: allow faking the time
            Some(meta) => match meta.policy.before_request(&req, SystemTime::now()) {
                BeforeRequest::Fresh(_) => {
                    self.fetch_remote_cache(key)?
                        .and_then(|(meta, image_data)| {
                            match meta.policy.before_request(&req, SystemTime::now()) {
                                // Return the fresh data
                                BeforeRequest::Fresh(_) => Some(CacheCheck::Fresh(image_data)),
                                // Went stale between checking meta vs getting the data
                                BeforeRequest::Stale { request, .. } => {
                                    Some(CacheCheck::Stale(request))
                                }
                            }
                        })
                }
                BeforeRequest::Stale { request, .. } => Some(CacheCheck::Stale(request)),
            },
        };

        Ok(maybe_check)
    }

    pub fn fetch_remote_meta(&self, key: &RemoteKey) -> anyhow::Result<Option<RemoteMeta>> {
        todo!();
    }

    pub fn fetch_remote_cache(
        &self,
        key: &RemoteKey,
    ) -> anyhow::Result<Option<(RemoteMeta, ImageData)>> {
        let read_txn = self.0.begin_read()?;
        // let meta_table = read_txn.open_table(METADATA_TABLE)?;
        // let maybe_meta = meta_table.get(key)?.map(|entry| entry.value());
        // TODO: check the probe against the stored meta:
        //
        // - If the cache is fresh then return the meta and image data
        // - If the cache is stale and there's and e-tag then send the etag with the request to
        //   potentially skip transferring the body
        // - Otherwise fetch the image from source (either local or remote) and store relevant info
        //
        // Notably we should ignore caching images from local sources, but I'm not sure how
        // accurately we can determine that (although it's probably easy to get good enough to
        // work)
        todo!();
    }

    pub fn run_garbage_collector(&self) -> anyhow::Result<()> {
        // TODO: pass over and remove entries and then run compaction. Can get the size of various
        // parts of the image data table to determine when we should actually run compaction
        // (things generally run better when there are pages that can be reused instead of always
        // compacting down to the minimal size)
        todo!();
    }
}

pub enum CacheCheck {
    Fresh(ImageData),
    Stale(request::Parts),
}
