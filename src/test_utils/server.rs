use std::{
    collections::{btree_map, BTreeMap},
    hash::Hasher,
    sync::{mpsc::Sender, Arc},
    thread,
    time::Duration,
};

use super::image::Sample;
use crate::{debug_impls::DebugBytesPrefix, image::cache::RemoteKey};

use http::{header, HeaderMap, HeaderValue};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use smart_debug::SmartDebug;
use tiny_http::{Header, Method, Request, Response, ResponseBox, Server};
use twox_hash::XxHash64;

type HandlerFn = fn(&State, &Request, &str) -> ResponseBox;

static META_SERVER: Lazy<MetaServer> = Lazy::new(|| {
    let server = Server::http("127.0.0.1:0").unwrap();

    let ip = server
        .server_addr()
        .to_ip()
        .expect("Provided addr is an ip");
    // We're using an `::http()` server
    let base_url = format!("http://{ip}");
    let slots = RwLock::default();

    spawn_router(server);

    MetaServer { base_url, slots }
});

pub fn spawn(state: State, handler_fn: HandlerFn) -> MiniServerHandle {
    let state = SharedState::new(state.into());
    let mini_server = MiniServer {
        handler_fn,
        state: state.clone(),
    };
    let mut meta = META_SERVER.slots.write();
    let index = meta.len();
    meta.push(Some(mini_server));

    let base_url = META_SERVER.base_url.clone();
    let url = format!("{base_url}/{index}");

    MiniServerHandle { url, index, state }
}

fn spawn_router(server: Server) {
    thread::Builder::new()
        .name("test-server-router".into())
        .spawn(move || {
            for req in server.incoming_requests() {
                // Run each request in its own thread to isolate potential panics
                thread::Builder::new()
                    .name("test-server-handler".into())
                    .spawn(move || {
                        let resp =
                            try_respond(&req).unwrap_or_else(|| Response::empty(404).boxed());
                        let _ = req.respond(resp);
                    })
                    .unwrap();
            }
        })
        .unwrap();
}

fn try_respond(req: &Request) -> Option<ResponseBox> {
    let url = req.url();
    let trimmed = url.trim_start_matches('/');
    let delim_index = trimmed.find('/');
    let (server_index, subserver_url) = match delim_index {
        Some(index) => trimmed.split_at(index),
        None => (trimmed, ""),
    };
    let server_index: usize = server_index.parse().ok()?;

    let meta = META_SERVER.slots.read();
    let MiniServer { state, handler_fn } = meta.get(server_index)?.as_ref()?;
    let resp = {
        let state = state.read();
        (handler_fn)(&*state, req, subserver_url)
    };
    Some(resp)
}

struct MetaServer {
    base_url: String,
    slots: RwLock<Vec<Option<MiniServer>>>,
}

pub struct MiniServerHandle {
    url: String,
    index: usize,
    state: SharedState,
}

impl MiniServerHandle {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn mount_image<F: Into<File>>(&self, file: F) -> RemoteKey {
        let file = file.into();
        let mut state = self.state.write();
        let mut num_files = state.files.len();
        let new_name = loop {
            let new_name = format!(
                "/file_{}{}",
                num_files,
                file.mime.to_ext().unwrap_or(".unknown")
            );
            if !state.files.contains_key(&new_name) {
                break new_name;
            } else {
                num_files += 1;
            }
        };

        let full_url = format!("{}{}", self.url(), new_name);
        let key = RemoteKey::new_unchecked(full_url);
        state.files.insert(new_name, file);
        key
    }

    pub fn swap_image<F: Into<File>>(&self, key: &RemoteKey, file: F) -> Option<()> {
        let file = file.into();
        let url = key.get();
        let rel_path = url.strip_prefix(self.url())?;
        let mut state = self.state.write();
        match state.files.entry(rel_path.to_owned()) {
            btree_map::Entry::Vacant(_) => None,
            btree_map::Entry::Occupied(mut slot) => {
                slot.insert(file);
                Some(())
            }
        }
    }
}

impl Drop for MiniServerHandle {
    fn drop(&mut self) {
        let mut meta = META_SERVER.slots.write();
        if let Some(entry) = meta.get_mut(self.index) {
            *entry = None;
        }
    }
}

struct MiniServer {
    handler_fn: HandlerFn,
    state: SharedState,
}

type SharedState = Arc<RwLock<State>>;

#[derive(Default)]
pub struct State {
    files: BTreeMap<String, File>,
    send: Option<Sender<FromServer>>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn file(mut self, url_path: String, file: File) -> Self {
        self.files.insert(url_path, file);
        self
    }

    pub fn send(mut self, send: Sender<FromServer>) -> Self {
        self.send = Some(send);
        self
    }

    pub fn send_msg(&self, msg: FromServer) {
        let _ = self.send.as_ref().unwrap().send(msg);
    }
}

pub enum FromServer {
    UserAgent(Option<String>),
}

// TODO: split out some of this logic into some cache control test server crate? There's a lot of
// low-level cache control server side stuff that we wind up implementing just to test things
/// Spin up a server, so we can test network requests without external services
pub fn mock_file_server(files: Vec<(String, File)>) -> MiniServerHandle {
    let files = files
        .into_iter()
        .map(|(path, file)| (path, file.into()))
        .collect();
    let state = State { files, send: None };
    spawn(state, |state, req, req_url| {
        if *req.method() != Method::Get {
            return Response::empty(404).boxed();
        }

        let Some(file) = state.files.get(req_url) else {
            return Response::empty(404).boxed();
        };

        // <https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/ETag#caching_of_unchanged_resources>
        //
        // > The server compares the client's `ETag` (sent with `If-None-Match`) with the
        // > `ETag` for its current version of the resource, and if both values match (that
        // > is, the resource has not changed), the server sends back a `304 Not Modified`
        // > status, without a body, which tells the client that the cached version of the
        // > response is still good to use (fresh).
        let desired_header_name: tiny_http::HeaderField =
            http::header::IF_NONE_MATCH.as_str().parse().unwrap();
        let maybe_client_etag = req.headers().iter().find_map(|header| {
            (header.field == desired_header_name).then(|| header.value.to_string())
        });
        match (file.include_etag, maybe_client_etag.as_deref()) {
            (true, Some(client_etag)) => {
                let body_hash = hash(&file.bytes);
                let server_etag = format!("\"{body_hash:x}\"");
                if server_etag == client_etag {
                    let header_name = http::header::ETAG.as_str().as_bytes();
                    let header =
                        Header::from_bytes(header_name, server_etag.as_bytes())
                            .unwrap();
                    Response::empty(http::status::StatusCode::NOT_MODIFIED.as_u16())
                        .with_header(header)
                        .boxed()
                } else {
                    file.to_owned().into()
                }
            }
            _ => file.to_owned().into(),
        }
    })
}

#[derive(Clone, Copy, Debug)]
pub struct CacheControl {
    immutable: bool,
    max_age: Option<Duration>,
    no_store: bool,
    private: bool,
}

impl CacheControl {
    // Same as what `derive(Default)` would do, but const
    pub const fn new() -> Self {
        Self {
            immutable: false,
            max_age: None,
            no_store: false,
            private: false,
        }
    }

    pub const fn immutable(mut self) -> Self {
        self.immutable = true;
        self
    }

    pub const fn max_age(mut self, age: Duration) -> Self {
        self.max_age = Some(age);
        self
    }

    pub const fn no_store(mut self) -> Self {
        self.no_store = true;
        self
    }

    pub const fn private(mut self) -> Self {
        self.private = true;
        self
    }

    fn to_header_value(&self) -> Option<String> {
        let CacheControl {
            immutable,
            max_age,
            no_store,
            private,
        } = self;
        let mut cache_control = Vec::new();
        if *immutable {
            cache_control.push("immutable".to_owned());
        }
        if let Some(age) = max_age {
            cache_control.push(format!("max-age={}", age.as_secs()));
        }
        if *no_store {
            cache_control.push("no-store".to_owned());
        }
        if *private {
            cache_control.push("private".to_owned());
        }

        if !cache_control.is_empty() {
            let cc = cache_control.join(", ");
            cc.parse().ok()
        } else {
            None
        }
    }
}

impl From<CacheControl> for Header {
    fn from(cache_control: CacheControl) -> Self {
        let value = cache_control.to_header_value().unwrap();
        Self::from_bytes(header::CACHE_CONTROL.as_str(), value).unwrap()
    }
}

impl From<CacheControl> for HeaderMap {
    fn from(cache_control: CacheControl) -> Self {
        let mut map = HeaderMap::new();

        if let Some(value) = cache_control.to_header_value() {
            let value = HeaderValue::from_str(&value).unwrap();
            map.insert(header::CACHE_CONTROL, value);
        }

        map
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ContentType {
    Gif,
    Jpg,
    Png,
    Qoi,
    Svg,
    Webp,
    Other(&'static str),
}

impl From<Sample> for ContentType {
    fn from(sample: Sample) -> Self {
        match sample {
            Sample::Gif(_) => Self::Gif,
            Sample::Jpg(_) => Self::Jpg,
            Sample::Png(_) => Self::Png,
            Sample::Qoi(_) => Self::Qoi,
            Sample::Svg(_) => Self::Svg,
            Sample::Webp(_) => Self::Webp,
        }
    }
}

impl ContentType {
    fn to_str(self) -> &'static str {
        match self {
            Self::Gif => "image/gif",
            Self::Jpg => "image/jpeg",
            Self::Png => "image/png",
            Self::Qoi => "image/qoi",
            Self::Svg => "image/svg+xml",
            Self::Webp => "image/webp",
            Self::Other(other) => other,
        }
    }

    fn to_ext(self) -> Option<&'static str> {
        match self {
            Self::Gif => Some(".gif"),
            Self::Jpg => Some(".jpeg"),
            Self::Png => Some(".png"),
            Self::Qoi => Some(".qoi"),
            Self::Svg => Some(".svg"),
            Self::Webp => Some(".webp"),
            Self::Other(_) => None,
        }
    }
}

impl From<ContentType> for Header {
    fn from(content_ty: ContentType) -> Self {
        let header_name = header::CONTENT_TYPE.as_str().as_bytes();
        let content_ty = content_ty.to_str().as_bytes();
        Header::from_bytes(header_name, content_ty).unwrap()
    }
}

#[derive(Clone, SmartDebug)]
pub struct File {
    pub mime: ContentType,
    pub cache_control: Option<CacheControl>,
    pub include_etag: bool,
    #[debug(wrapper = DebugBytesPrefix)]
    pub bytes: Vec<u8>,
}

impl File {
    pub fn new(mime: ContentType, cache_control: Option<CacheControl>, bytes: &[u8]) -> Self {
        Self {
            mime,
            cache_control,
            include_etag: false,
            bytes: bytes.into(),
        }
    }
}

fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = XxHash64::default();
    hasher.write(bytes);
    hasher.finish()
}

impl From<File> for ResponseBox {
    fn from(file: File) -> Self {
        let File {
            mime,
            cache_control,
            include_etag,
            bytes,
        } = file;

        let body_hash = hash(&bytes);
        let mut resp = Response::from_data(bytes).with_header(mime);

        if let Some(c_c) = cache_control {
            resp.add_header(c_c);
        }

        if include_etag {
            let header_name = http::header::ETAG.as_str().as_bytes();
            let header_val = format!("\"{body_hash:x}\"");
            let header = Header::from_bytes(header_name, header_val.as_bytes()).unwrap();
            resp.add_header(header);
        }

        resp.boxed()
    }
}
