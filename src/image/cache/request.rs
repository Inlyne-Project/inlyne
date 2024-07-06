// TODO: rename this module to http?

use std::{io::Read, str::FromStr, sync::OnceLock};

use super::RemoteKey;

use http::{header, request, HeaderMap, HeaderName, HeaderValue, StatusCode};
use http_cache_semantics::{RequestLike, ResponseLike};

pub fn http_call_req(req: ureq::Request) -> anyhow::Result<(StandardResp, Vec<u8>)> {
    tracing::debug!(?req, "Fetching remote image");

    const BODY_SIZE_LIMIT: usize = 20 * 1_024 * 1_024;

    let resp = req.call()?;
    let standard_resp = (&resp).into();
    let mut body = Vec::new();
    resp.into_reader()
        .take(u64::try_from(BODY_SIZE_LIMIT).unwrap())
        .read_to_end(&mut body)?;
    Ok((standard_resp, body))
}

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
            " https://github.com/Inlyne-Project/inlyne"
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

pub struct StandardResp {
    code: StatusCode,
    headers: HeaderMap,
}

impl From<&ureq::Response> for StandardResp {
    fn from(resp: &ureq::Response) -> Self {
        let code = StatusCode::from_u16(resp.status()).unwrap();
        let mut headers = HeaderMap::new();
        for header_name in resp.headers_names() {
            if let Some(header_val) = resp.header(&header_name) {
                let header_name: HeaderName = header_name.parse().unwrap();
                let header_val = HeaderValue::from_str(header_val).unwrap();
                // NOTE: append not insert because headers are a multi-map
                headers.append(header_name, header_val);
            }
        }

        Self { code, headers }
    }
}

impl ResponseLike for StandardResp {
    fn status(&self) -> StatusCode {
        self.code
    }

    fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}
