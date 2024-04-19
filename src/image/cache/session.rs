use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::image::ImageData;

use http_cache_semantics::CachePolicy;
use parking_lot::RwLock;
use url::Url;

use super::RemoteKey;

#[derive(Default)]
pub struct Cache {
    local: RwLock<BTreeMap<PathBuf, (SystemTime, ImageData)>>,
    remote: RwLock<BTreeMap<RemoteKey, (CachePolicy, ImageData)>>,
}

impl Cache {
    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn fetch_local_cached(&self, local: PathBuf) -> anyhow::Result<ImageData> {
        let Some(m_time) = fs::metadata(&local).and_then(|meta| meta.modified()).ok()
        // Fallback to always refetching when we can't read the mtime
        else {
            return Self::fetch_local(&local);
        };

        {
            if let Some((stored, image_data)) = self.local.read().get(&local) {
                if *stored == m_time {
                    return Ok(image_data.to_owned());
                }
            }
        }

        {
            let image_data = Self::fetch_local(&local)?;
            self.local
                .write()
                .insert(local, (m_time, image_data.clone()));
            Ok(image_data)
        }
    }

    pub fn fetch_local(local: &Path) -> anyhow::Result<ImageData> {
        todo!();
    }

    pub fn check_remote_cache(&self, remote: &RemoteKey) -> Option<RemoteEntry> {
        self.remote.read().get(remote).map(|(policy, data)| todo!())
    }
}

pub enum RemoteEntry {
    Fresh(ImageData),
    Stale(CachePolicy),
}
