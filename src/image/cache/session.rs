use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::{RemoteKey, StandardRequest};
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

    pub fn fetch_local(local: &Path) -> anyhow::Result<ImageData> {
        let contents = fs::read(local)?;
        // TODO: need to try loading as an svg too to mimic current behavior
        ImageData::load(&contents, true)
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
}

pub enum RemoteEntry {
    Fresh(ImageData),
    Stale(request::Parts),
}
