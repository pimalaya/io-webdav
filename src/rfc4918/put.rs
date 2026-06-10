//! Generic `PUT` coroutine (RFC 4918 §9.7).
//!
//! Sends a `PUT` against `path` with the caller-supplied body bytes
//! and content type. Stays byte-oriented: callers parse iCal/vCard
//! upstream.
//!
//! Supports the optional `If-Match` (RFC 9110 §13.1.1) and
//! `If-None-Match` (RFC 9110 §13.1.2) preconditions so callers can
//! gate the write on a known ETag.

use alloc::{string::String, vec::Vec};

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    send::{SendRaw, SendResult},
};

/// Coroutine that runs a `PUT`.
#[derive(Debug)]
pub struct Put(SendRaw);

impl Put {
    /// Builds a new `PUT` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
        if_match: Option<&str>,
        if_none_match: Option<&str>,
    ) -> Self {
        let mut builder = WebdavRequest::put(base_url, auth, user_agent, path)
            .content_type(content_type);

        if let Some(etag) = if_match {
            builder = builder.if_match(etag);
        }

        if let Some(etag) = if_none_match {
            builder = builder.if_none_match(etag);
        }

        let request = builder.body(body);
        Self(SendRaw::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}

/// Reads the `ETag` header (RFC 9110 §8.8.3) out of an HTTP response,
/// stripping the surrounding double quotes when present. Useful for
/// callers that want to thread the post-`PUT` ETag into their cache.
pub fn read_etag(response: &io_http::rfc9110::response::HttpResponse) -> Option<String> {
    response
        .header("etag")
        .map(|raw| raw.trim_matches('"').into())
}
