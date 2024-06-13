use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::{load_image, RemoteKey, StableImage, StandardRequest};
use crate::image::ImageData;

use http::request;
use http_cache_semantics::{BeforeRequest, CachePolicy};
use parking_lot::RwLock;

#[derive(Default)]
pub struct Cache {
    local: RwLock<BTreeMap<PathBuf, (SystemTime, ImageData)>>,
    remote: RwLock<BTreeMap<RemoteKey, (CachePolicy, ImageData)>>,
}

impl Cache {
    pub fn fetch_local_cached(&self, local: &Path) -> Option<ImageData> {
        let Some(m_time) = fs::metadata(local).and_then(|meta| meta.modified()).ok()
        // Fallback to always refetching when we can't read the mtime
        else {
            return None;
        };

        {
            if let Some((stored, image_data)) = self.local.read().get(local) {
                if *stored == m_time {
                    return Some(image_data.to_owned());
                }
            }
        }

        None
    }

    pub fn fetch_local(&self, path: &Path) -> anyhow::Result<(SystemTime, StableImage)> {
        let contents = fs::read(path)?;
        let m_time = fs::metadata(path)?.modified()?;
        let image = load_image(&contents)?;
        Ok((m_time, image))
    }

    pub fn check_remote_cache(&self, remote: &RemoteKey) -> Option<RemoteEntry> {
        self.remote.read().get(remote).map(|(policy, image_data)| {
            let req: StandardRequest = remote.into();
            // TODO: allow for faking time here
            match policy.before_request(&req, SystemTime::now()) {
                BeforeRequest::Fresh(_) => RemoteEntry::Fresh(image_data.to_owned()),
                BeforeRequest::Stale { request, .. } => RemoteEntry::Stale(request),
            }
        })
    }

    pub fn insert_local(&self, path: PathBuf, val: (SystemTime, ImageData)) {
        let mut local_cache = self.local.write();
        local_cache.insert(path, val);
    }

    pub fn insert_remote(&self, remote: RemoteKey, val: (CachePolicy, ImageData)) {
        let mut remote_cache = self.remote.write();
        remote_cache.insert(remote, val);
    }
}

pub enum RemoteEntry {
    Fresh(ImageData),
    Stale(request::Parts),
}
