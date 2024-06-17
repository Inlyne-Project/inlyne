use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    image::cache::{global::RemoteMeta, RemoteKey, StableImage},
    utils,
};

use anyhow::Context;
use http_cache_semantics::CachePolicy;
use rusqlite::{types::FromSqlError, Connection};

use super::wrappers::{CachePolicyBytes, StableImageBytes, SystemTimeSecs};

// TODO: _a lot_ more sql impls to clean things up

/// The current version for our database file
///
/// We're a cache so we don't really have to keep worrying about preserving data permanently. If we
/// want to make some really nasty changes without dealing with migrations then we can bump this
/// version and rotate to a totally new file entirely. Old versions are handled durring garbage
/// collection
const VERSION: u32 = 0;

fn file_name() -> String {
    format!("image-cache-v{VERSION}.db3")
}

fn db_path() -> anyhow::Result<PathBuf> {
    let cache_dir = utils::inlyne_cache_dir().context("Failed to locate cache dir")?;
    let db_path = cache_dir.join(file_name());
    Ok(db_path)
}

const SCHEMA: &str = include_str!("db_schema.sql");

// TODO: create a connection pool so that we can actually re-use connections (and their cache)
// instead of having to create a new one for each worker or serialize all cache interactions
pub struct Db(Connection);

impl Db {
    pub fn load() -> anyhow::Result<Self> {
        let cache_dir = utils::inlyne_cache_dir().context("Unable to locate cache dir")?;
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache dir at: {}", cache_dir.display()))?;
        let db_path = db_path()?;
        Self::load_from_file(&db_path)
    }

    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        Self::create_schema(&conn)?;
        Ok(Self(conn))
    }

    #[cfg(test)]
    pub fn load_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::create_schema(&conn)?;
        Ok(Self(conn))
    }

    fn create_schema(conn: &Connection) -> anyhow::Result<()> {
        conn.execute(SCHEMA, ())?;
        Ok(())
    }

    pub fn get_meta(&self, remote: &RemoteKey) -> anyhow::Result<Option<RemoteMeta>> {
        let mut stmt = self
            .0
            .prepare_cached("select generation, last_used, policy from images where url = ?1")?;
        let mut meta_iter = stmt.query_map([&remote.0], |row| {
            let generation = row.get(0)?;
            let last_used = row.get::<_, SystemTimeSecs>(1)?.into();
            let policy = (&row.get::<_, CachePolicyBytes>(2)?)
                .try_into()
                .map_err(|err| FromSqlError::Other(Box::new(err)))?;
            Ok(RemoteMeta {
                generation,
                last_used,
                policy,
            })
        })?;
        meta_iter.next().transpose().map_err(Into::into)
    }

    pub fn get_data(
        &self,
        remote: &RemoteKey,
        generation: u32,
    ) -> anyhow::Result<Option<StableImage>> {
        let mut stmt = self
            .0
            .prepare_cached("select image from images where url = ?1 and generation = ?2")?;
        let mut data_iter = stmt.query_map((&remote.0, generation), |row| {
            // TODO: fixup the error type here to be more sane
            let blah = row
                .get::<_, StableImageBytes>(0)?
                .try_into()
                .map_err(|err| {
                    tracing::warn!("Corrupt stable-image: {err}");
                    FromSqlError::InvalidType
                })?;
            Ok(blah)
        })?;
        data_iter.next().transpose().map_err(Into::into)
    }

    pub fn insert(
        &mut self,
        remote: &RemoteKey,
        policy: &CachePolicy,
        image: StableImage,
    ) -> anyhow::Result<()> {
        let url = &remote.0;
        let now: SystemTimeSecs = SystemTime::now().try_into()?;
        let policy: CachePolicyBytes = policy.try_into()?;
        let image: StableImageBytes = image.into();

        let txn = self.0.transaction()?;
        let next_gen = {
            let mut stmt = txn.prepare_cached("select generation from images where url = ?1")?;
            let mut gen_iter = stmt.query_map([url], |row| row.get(0).map_err(Into::into))?;
            gen_iter.next().transpose()?.unwrap_or(0u32).wrapping_add(1)
        };
        {
            // TODO: change this query to handle existing entries
            let mut stmt = txn.prepare_cached(
                "insert into images (url, generation, last_used, policy, image)
                    values (?1, ?2, ?3, ?4, ?5)",
            )?;
            stmt.execute((url, next_gen, now, policy, image))?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn refresh(
        &self,
        remote: &RemoteKey,
        generation: u32,
        policy: &CachePolicy,
    ) -> anyhow::Result<()> {
        todo!();
    }

    pub fn refresh_last_used(&self, remote: &RemoteKey, generation: u32) -> anyhow::Result<()> {
        let url = &remote.0;
        let now: SystemTimeSecs = SystemTime::now().try_into()?;
        self.0.execute(
            "update images set last_used = ?1 where url = ?2 and generation = ?3",
            (now, url, generation),
        )?;
        Ok(())
    }
}
