use std::{
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    thread,
};

use tiny_http::{Header, Method, Request, Response, Server};
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
pub fn mock_file_server(files: Vec<File>) -> (FileServer, String) {
    let server = FileServer::spawn(files);
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

pub struct FileServer {
    url: String,
    shutdown_send: SyncSender<()>,
}

impl FileServer {
    // Spawn the server
    // |-> Move one handle to a shutdown thread
    // |-> Move The other handle to a request handler thread
    // |   \-> Each request gets handled on a newly spawned thread
    // \-> Return a server guard that shuts down on `drop()`
    pub fn spawn<Files: Into<Arc<[File]>>>(files: Files) -> Self {
        let files = files.into();
        // Bind to the ephemeral port and then get the actual resolved address
        let server = Server::http("127.0.0.1:0").unwrap();
        let ip = server
            .server_addr()
            .to_ip()
            .expect("Provided addr is an ip");
        // We're using an `::http()` server
        let url = format!("http://{ip}");

        let server = Arc::new(server);
        let (shutdown_send, shutdown_recv) = sync_channel(1);

        Self::spawn_router(Arc::clone(&server), files);
        Self::spawn_shutdown(server, shutdown_recv);

        Self { url, shutdown_send }
    }

    fn spawn_shutdown(server: Arc<Server>, shutdown_recv: Receiver<()>) {
        thread::spawn(move || {
            if let Ok(()) = shutdown_recv.recv() {
                // Unblock the `.incoming_requests()`
                server.unblock();
            }
        });
    }

    fn spawn_router(server: Arc<Server>, files: Arc<[File]>) {
        thread::spawn(move || {
            for req in server.incoming_requests() {
                let req_files = Arc::clone(&files);
                thread::spawn(|| Self::handle_req(req, req_files));
            }
            // Time to shutdown now
        });
    }

    fn handle_req(req: Request, files: Arc<[File]>) {
        match req.method() {
            Method::Get => {
                let path = req.url();
                match files.iter().find(|file| file.url_path == path) {
                    Some(file) => {
                        let header =
                            Header::from_bytes(b"Content-Type", file.mime.as_bytes()).unwrap();
                        let resp = Response::from_data(file.bytes.clone()).with_header(header);
                        let _ = req.respond(resp);
                    }
                    None => _ = req.respond(Response::empty(404)),
                }
            }
            _ => _ = req.respond(Response::empty(404)),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for FileServer {
    fn drop(&mut self) {
        let _ = self.shutdown_send.send(());
    }
}
