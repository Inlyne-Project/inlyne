use std::{cmp::Ordering, fs};

use super::{Key, Validation, ValidationProbe};
use crate::{image::ImageData, utils};

use anyhow::Context;
use redb::{backends::InMemoryBackend, Database, TableDefinition};

mod value_impls;

// Access to metadata should be fast, so we keep it in a separate table to avoid loading bulky
// image data when we don't need it
const METADATA_TABLE: TableDefinition<Key, Validation> = TableDefinition::new("metadata_table");
const DATA_TABLE: TableDefinition<Key, ImageData> = TableDefinition::new("data_table");

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

    pub fn in_memory() -> anyhow::Result<Self> {
        let backend = InMemoryBackend::new();
        let db = Database::builder().create_with_backend(backend)?;
        Ok(Self(db))
    }

    pub fn fetch_cached(
        &self,
        key: &Key<'static>,
        probe: &ValidationProbe,
    ) -> anyhow::Result<(Validation, ImageData)> {
        todo!();
    }

    pub fn run_garbage_collector(&self) -> anyhow::Result<()> {
        // TODO: pass over and remove entries and then run compaction
        todo!();
    }
}
