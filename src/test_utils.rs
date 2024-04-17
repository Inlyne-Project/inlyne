use std::{sync::Arc, thread};

use tiny_http::{Header, Method, Request, Response, ResponseBox, Server};
use tracing_subscriber::prelude::*;

pub fn init_test_log() {
    let filter = tracing_subscriber::filter::Targets::new()
        .with_default(tracing_subscriber::filter::LevelFilter::WARN)
        .with_target("inlyne", tracing_subscriber::filter::LevelFilter::TRACE);
    // Ignore errors because other tests in the same binary may have already initialized the logger
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_test_writer(),
        )
        .try_init();
}

/// Spin up a server, so we can test network requests without external services
pub fn mock_file_server(files: Vec<File>) -> (HttpServer, String) {
    let files: Arc<[File]> = files.into();
    let server = HttpServer::spawn(files, |files, req| match req.method() {
        Method::Get => {
            let path = req.url();
            match files.iter().find(|file| file.url_path == path) {
                Some(file) => {
                    let header = Header::from_bytes(b"Content-Type", file.mime.as_bytes()).unwrap();
                    Response::from_data(file.bytes.clone())
                        .with_header(header)
                        .boxed()
                }
                None => Response::empty(404).boxed(),
            }
        }
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

// TODO: move some of this to a `tiny-http-utils` crate?
pub struct HttpServer {
    url: String,
    server: Arc<Server>,
}

impl HttpServer {
    // Spawn the server
    // |-> Move a handle to a request handler thread
    // |   \-> Each request gets handled on a newly spawned thread
    // \-> Return a server guard that shuts down on `drop()`
    pub fn spawn<S, F>(state: S, handler_fn: F) -> Self
    where
        S: Send + Clone + 'static,
        F: Fn(S, &Request) -> ResponseBox + Send + Clone + Copy + 'static,
    {
        // let files = files.into();
        // Bind to the ephemeral port and then get the actual resolved address
        let server = Server::http("127.0.0.1:0").unwrap();
        let ip = server
            .server_addr()
            .to_ip()
            .expect("Provided addr is an ip");
        // We're using an `::http()` server
        let url = format!("http://{ip}");

        let server = Arc::new(server);

        Self::spawn_router(Arc::clone(&server), state, handler_fn);

        Self { url, server }
    }

    fn spawn_router<S, F>(server: Arc<Server>, state: S, handler_fn: F)
    where
        S: Send + Clone + 'static,
        F: Fn(S, &Request) -> ResponseBox + Send + Clone + Copy + 'static,
    {
        thread::spawn(move || {
            for req in server.incoming_requests() {
                let s2 = state.clone();
                thread::spawn(move || {
                    let resp = handler_fn(s2, &req);
                    let _ = req.respond(resp);
                });
            }
            // Time to shutdown now
        });
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        // Unblock the `.incoming_requests()`
        self.server.unblock();
    }
}
