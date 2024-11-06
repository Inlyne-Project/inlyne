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
use rusqlite::{types::FromSqlError, Connection, OptionalExtension};

use super::wrappers::{CachePolicyBytes, StableImageBytes, SystemTimeSecs};

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

const SCHEMA: &str = include_str!("db_schema.sql");

// TODO: create a connection pool so that we can actually re-use connections (and their cache)
// instead of having to create a new one for each worker or serialize all cache interactions
pub struct Db(Connection);

impl Db {
    pub fn default_path() -> anyhow::Result<PathBuf> {
        let cache_dir = utils::inlyne_cache_dir().context("Failed to locate cache dir")?;
        let db_path = cache_dir.join(file_name());
        Ok(db_path)
    }

    pub fn open_or_create(path: &Path) -> anyhow::Result<Self> {
        let db_dir = path.parent().with_context(|| {
            format!(
                "Unable to locate database directory from: {}",
                path.display()
            )
        })?;
        fs::create_dir_all(db_dir)
            .with_context(|| format!("Failed creating db directory at: {}", db_dir.display()))?;
        let conn = Connection::open(path)?;
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
        stmt.query_row([&remote.0], |row| {
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
        })
        .optional()
        .map_err(Into::into)
    }

    pub fn get_data(
        &self,
        remote: &RemoteKey,
        generation: u32,
    ) -> anyhow::Result<Option<StableImage>> {
        let mut stmt = self
            .0
            .prepare_cached("select image from images where url = ?1 and generation = ?2")?;
        stmt.query_row((&remote.0, generation), |row| {
            let blah = row
                .get::<_, StableImageBytes>(0)?
                .try_into()
                .map_err(|err| FromSqlError::Other(Box::new(err)))?;
            Ok(blah)
        })
        .optional()
        .map_err(Into::into)
    }

    pub fn insert(
        &mut self,
        remote: &RemoteKey,
        policy: &CachePolicy,
        image: StableImage,
        now: SystemTime,
    ) -> anyhow::Result<()> {
        let url = &remote.0;
        let now: SystemTimeSecs = now.try_into()?;
        let policy: CachePolicyBytes = policy.try_into()?;
        let image: StableImageBytes = image.into();

        let mut stmt = self.0.prepare_cached(
            "insert or replace into images (url, last_used, policy, image, generation)
                values (?1, ?2, ?3, ?4, abs(random() % 1000000))",
        )?;
        stmt.execute((url, now, policy, image))?;
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

    pub fn refresh_last_used(
        &self,
        remote: &RemoteKey,
        generation: u32,
        now: SystemTime,
    ) -> anyhow::Result<()> {
        let url = &remote.0;
        let now: SystemTimeSecs = now.try_into()?;
        // TODO: cache this query
        self.0.execute(
            "update images set last_used = ?1 where url = ?2 and generation = ?3",
            (now, url, generation),
        )?;
        Ok(())
    }
}
