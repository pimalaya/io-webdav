//! WebDAV request builder.
//!
//! Wraps [`io_http::rfc9110::request::HttpRequest`] with the WebDAV
//! method shortcuts (`PROPFIND`, `PROPPATCH`, `MKCOL`, `REPORT`,
//! `COPY`, `MOVE`, `OPTIONS`) plus the `Depth`, `Destination`,
//! `Overwrite`, `If-Match`, `If-None-Match` and content-type headers
//! every CalDAV/CardDAV coroutine touches.
//!
//! Builds on [`url::Url::join`] for path composition via
//! `resolve`.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use io_http::rfc9110::request::HttpRequest;
use log::trace;
use url::Url;

use crate::rfc4918::{WebdavAuth, emit_header, resolve};

/// Fluent builder for a WebDAV HTTP request.
#[derive(Clone, Debug)]
pub struct WebdavRequest {
    inner: HttpRequest,
}

impl WebdavRequest {
    /// Builds a request targeting `path` (relative to `base_url`) with
    /// the given HTTP method. Sets `Host` from `base_url` and the
    /// optional `Authorization` header from `auth`. `user_agent` is
    /// emitted as the `User-Agent` header.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        method: &str,
        path: &str,
    ) -> Self {
        let url = resolve(base_url, path);

        let host = match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            (None, _) => String::new(),
        };

        let mut inner = HttpRequest::get(url).header("User-Agent", user_agent);

        if !host.is_empty() {
            inner = inner.header("Host", host);
        }

        if let Some(value) = emit_header(auth) {
            inner = inner.header("Authorization", value);
        }

        inner.method = method.to_string();

        Self { inner }
    }

    /// Builds a `GET` request.
    pub fn get(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "GET", path)
    }

    /// Builds a `DELETE` request.
    pub fn delete(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "DELETE", path)
    }

    /// Builds a `PUT` request.
    pub fn put(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "PUT", path)
    }

    /// Builds an `OPTIONS` request.
    pub fn options(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "OPTIONS", path)
    }

    /// Builds a `MKCOL` request (RFC 4918 §9.3).
    pub fn mkcol(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "MKCOL", path)
    }

    /// Builds a `PROPFIND` request (RFC 4918 §9.1).
    pub fn propfind(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "PROPFIND", path)
    }

    /// Builds a `PROPPATCH` request (RFC 4918 §9.2).
    pub fn proppatch(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "PROPPATCH", path)
    }

    /// Builds a `REPORT` request (RFC 3253 §3.6).
    pub fn report(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "REPORT", path)
    }

    /// Builds a `COPY` request (RFC 4918 §9.8).
    pub fn copy(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "COPY", path)
    }

    /// Builds a `MOVE` request (RFC 4918 §9.9).
    pub fn r#move(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        Self::new(base_url, auth, user_agent, "MOVE", path)
    }

    /// Sets the `Depth` header (RFC 4918 §10.2).
    pub fn depth(mut self, depth: u8) -> Self {
        self.inner = self.inner.header("Depth", depth);
        self
    }

    /// Sets the `Destination` header (RFC 4918 §10.3).
    pub fn destination(mut self, destination: &str) -> Self {
        self.inner = self.inner.header("Destination", destination);
        self
    }

    /// Sets the `Overwrite` header (RFC 4918 §10.6) to `T` or `F`.
    pub fn overwrite(mut self, overwrite: bool) -> Self {
        let value = if overwrite { "T" } else { "F" };
        self.inner = self.inner.header("Overwrite", value);
        self
    }

    /// Sets the `If-Match` header (RFC 9110 §13.1.1) to the given ETag.
    pub fn if_match(mut self, etag: &str) -> Self {
        self.inner = self.inner.header("If-Match", entity_tag(etag));
        self
    }

    /// Sets the `If-None-Match` header (RFC 9110 §13.1.2) to the given
    /// ETag.
    pub fn if_none_match(mut self, etag: &str) -> Self {
        self.inner = self.inner.header("If-None-Match", entity_tag(etag));
        self
    }

    /// Sets the `Content-Type` header.
    pub fn content_type(mut self, value: &str) -> Self {
        self.inner = self.inner.header("Content-Type", value);
        self
    }

    /// Shortcut for `content_type("text/xml; charset=utf-8")`.
    pub fn content_type_xml(self) -> Self {
        self.content_type("text/xml; charset=utf-8")
    }

    /// Shortcut for `content_type("text/calendar; charset=utf-8")`.
    pub fn content_type_ical(self) -> Self {
        self.content_type("text/calendar; charset=utf-8")
    }

    /// Shortcut for `content_type("text/vcard; charset=utf-8")`.
    pub fn content_type_vcard(self) -> Self {
        self.content_type("text/vcard; charset=utf-8")
    }

    /// Finalizes the request with the given body and returns the
    /// underlying [`HttpRequest`] ready for [`crate::rfc4918::send`].
    ///
    /// Trace-logs the body: WebDAV request bodies are always UTF-8 text
    /// (XML, iCalendar or vCard), so io-webdav can safely render them,
    /// whereas io-http (which cannot know the content type) does not.
    pub fn body(mut self, body: Vec<u8>) -> HttpRequest {
        if !body.is_empty() {
            trace!("request body: {}", String::from_utf8_lossy(&body));
        }
        self.inner = self.inner.body(body);
        self.inner
    }
}

/// Formats an ETag as a conditional-header entity-tag (RFC 9110 §8.8.3):
/// a bare strong tag gets wrapped in double quotes; `*`, weak (`W/...`)
/// and already-quoted values pass through unchanged.
fn entity_tag(etag: &str) -> String {
    if etag == "*" || etag.starts_with('"') || etag.starts_with("W/") {
        etag.to_string()
    } else {
        format!("\"{etag}\"")
    }
}

#[cfg(test)]
mod tests {
    use io_http::rfc7617::basic::HttpAuthBasic;
    use url::Url;

    use crate::rfc4918::{WebdavAuth, request::*};

    fn base() -> Url {
        Url::parse("https://dav.example.org/dav/").unwrap()
    }

    #[test]
    fn empty_path_returns_base() {
        let req = WebdavRequest::propfind(&base(), &WebdavAuth::None, "io-webdav/test", "");
        let request = req.body(Vec::new());
        assert_eq!(request.url.as_str(), "https://dav.example.org/dav/");
    }

    #[test]
    fn absolute_path_replaces() {
        let req =
            WebdavRequest::propfind(&base(), &WebdavAuth::None, "io-webdav/test", "/principals/");
        let request = req.body(Vec::new());
        assert_eq!(request.url.as_str(), "https://dav.example.org/principals/");
    }

    #[test]
    fn relative_path_appends() {
        let req = WebdavRequest::propfind(&base(), &WebdavAuth::None, "io-webdav/test", "personal");
        let request = req.body(Vec::new());
        assert_eq!(request.url.as_str(), "https://dav.example.org/dav/personal");
    }

    #[test]
    fn auth_basic_emits_header() {
        let auth = WebdavAuth::Basic(HttpAuthBasic::new("alice", "secret"));
        let req = WebdavRequest::get(&base(), &auth, "io-webdav/test", "");
        let request = req.body(Vec::new());
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "Authorization" && value.starts_with("Basic "))
        );
    }
}
