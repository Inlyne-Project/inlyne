use std::{sync::mpsc::Sender, thread};

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use tiny_http::{Header, Method, Request, Response, ResponseBox, Server};

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
    let mini_server = MiniServer { handler_fn, state };
    let mut meta = META_SERVER.slots.write();
    let index = meta.len();
    meta.push(Some(mini_server));

    let base_url = META_SERVER.base_url.clone();
    let url = format!("{base_url}/{index}");

    MiniServerHandle { url, index }
}

fn spawn_router(server: Server) {
    thread::spawn(move || {
        for req in server.incoming_requests() {
            let resp = try_respond(&req).unwrap_or_else(|| Response::empty(404).boxed());
            let _ = req.respond(resp);
        }
    });
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
    let resp = (handler_fn)(state, req, subserver_url);
    Some(resp)
}

struct MetaServer {
    base_url: String,
    slots: RwLock<Vec<Option<MiniServer>>>,
}

pub struct MiniServerHandle {
    url: String,
    index: usize,
}

impl MiniServerHandle {
    pub fn url(&self) -> &str {
        &self.url
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
    state: State,
}

#[derive(Default)]
pub struct State {
    pub files: Vec<File>,
    pub send: Option<Sender<FromServer>>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn file(mut self, file: File) -> Self {
        self.files.push(file);
        self
    }

    pub fn send(mut self, send: Sender<FromServer>) -> Self {
        self.send = Some(send);
        self
    }
}

pub enum FromServer {
    UserAgent(Option<String>),
}

/// Spin up a server, so we can test network requests without external services
pub fn mock_file_server(files: Vec<File>) -> (MiniServerHandle, String) {
    let state = State { files, send: None };
    let server = spawn(state, |state, req, req_url| match req.method() {
        Method::Get => match state.files.iter().find(|file| file.url_path == req_url) {
            Some(file) => {
                let header = Header::from_bytes(b"Content-Type", file.mime.as_bytes()).unwrap();
                Response::from_data(file.bytes.clone())
                    .with_header(header)
                    .boxed()
            }
            None => Response::empty(404).boxed(),
        },
        _ => Response::empty(404).boxed(),
    });

    let url = server.url().to_owned();
    (server, url)
}

pub struct File {
    pub url_path: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

impl File {
    pub fn new(url_path: &str, mime: &str, bytes: &[u8]) -> Self {
        Self {
            url_path: url_path.to_owned(),
            mime: mime.to_owned(),
            bytes: bytes.to_owned(),
        }
    }
}
