use std::{cmp::Ordering, fs};

use super::{Key, Validation, ValidationProbe};
use crate::{image::ImageData, utils};

use anyhow::Context;
use redb::{backends::InMemoryBackend, Database, TableDefinition};

mod value_impls;

// TODO: separate remote and local validation types (separate key types too?) and then use an enum
// for the common `Key`
// Access to metadata should be fast, so we keep it in a separate table to avoid loading bulky
// image data when we don't need it
const LOCAL_META: TableDefinition<Key, Validation> = TableDefinition::new("inlyne-local-meta");
const REMOTE_META: TableDefinition<Key, Validation> = TableDefinition::new("inlyne-remote-meta");
const IMAGE_DATA: TableDefinition<Key, ImageData> = TableDefinition::new("inlyne-image-data");

// The database is currently externally versioned meaning that we switch to an entirely new file
// when we bump the version
// TODO: Garbage collection should also be adjusted to cleanup unused databases over time
const VERSION: u32 = 0;

fn db_name() -> String {
    format!("image-cache-v{VERSION}.redb")
}

impl<'a> redb::Key for Key<'a> {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        // Seems a bit odd to unwrap here, but it's what `redb` does for `&str`s internally...
        let data1 = std::str::from_utf8(data1).unwrap();
        let data2 = std::str::from_utf8(data2).unwrap();
        data1.cmp(&data2)
    }
}

pub fn run_garbage_collector() -> anyhow::Result<()> {
    let cache = Cache::load()?;
    cache.run_garbage_collector()
}

pub struct Cache(Database);

impl Cache {
    pub fn load() -> anyhow::Result<Self> {
        let cache_dir = utils::inlyne_cache_dir().context("Failed to locate cache dir")?;
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache dir at: {}", cache_dir.display()))?;
        let db_path = cache_dir.join(db_name());
        let db = Database::create(&db_path)
            .with_context(|| format!("Failed to create database at: {}", db_path.display()))?;
        Ok(Self(db))
    }

    pub fn in_memory() -> Self {
        let backend = InMemoryBackend::new();
        let db = Database::builder()
            .create_with_backend(backend)
            .expect("In-memory backend should be infallible");
        Self(db)
    }

    pub fn fetch_cached(
        &mut self,
        key: &Key<'static>,
        probe: ValidationProbe,
    ) -> anyhow::Result<(Validation, ImageData)> {
        let read_txn = self.0.begin_read()?;
        let meta_table = read_txn.open_table(METADATA_TABLE)?;
        let maybe_meta = meta_table.get(key)?.map(|entry| entry.value());
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
        // TODO: pass over and remove entries and then run compaction
        todo!();
    }
}
