use std::str::FromStr;
use std::sync::OnceLock;

use super::RemoteKey;

use http::{HeaderMap, request, header};
use http_cache_semantics::RequestLike;

/// Represents the very basic request parts that we always use
#[derive(Clone, Debug)]
pub struct StandardRequest {
    url: http::Uri,
}

impl From<&RemoteKey> for StandardRequest {
    fn from(key: &RemoteKey) -> Self {
        key.0
            .parse()
            .expect("Remote key should always be a valid url")
    }
}

impl FromStr for StandardRequest {
    type Err = http::uri::InvalidUri;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = s.parse()?;
        Ok(Self { url })
    }
}

impl From<&StandardRequest> for request::Parts {
    fn from(req: &StandardRequest) -> Self {
        let mut parts = request::Request::builder()
            .method(req.method())
            .uri(req.uri())
            .body(())
            .unwrap()
            .into_parts()
            .0;
        parts.headers = req.headers().to_owned();
        parts
    }
}

impl From<&StandardRequest> for ureq::Request {
    fn from(req: &StandardRequest) -> Self {
        let parts: request::Parts = req.into();
        parts.into()
    }
}

impl RequestLike for StandardRequest {
    fn uri(&self) -> http::Uri {
        self.url.clone()
    }

    fn method(&self) -> &http::Method {
        &http::Method::GET
    }

    fn headers(&self) -> &'static http::HeaderMap {
        static HEADERS: OnceLock<HeaderMap> = OnceLock::new();
        const DESCRIPTIVE_USER_AGENT: &str = concat!(
            "inlyne ",
            env!("CARGO_PKG_VERSION"),
            " https://github.com/trimental/inlyne"
        );
        HEADERS.get_or_init(|| {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::USER_AGENT,
                header::HeaderValue::from_static(DESCRIPTIVE_USER_AGENT),
            );
            headers
        })
    }

    fn is_same_uri(&self, other: &http::Uri) -> bool {
        &self.url == other
    }
}
