//! Generic `DELETE` coroutine (RFC 4918 §9.6).
//!
//! Sends a `DELETE` against `path`. Servers may return 204 No Content
//! (empty body) or a multistatus when the deletion partially failed;
//! callers inspect the response status to disambiguate.

use alloc::vec::Vec;

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    send::{SendRaw, SendResult},
};

/// Coroutine that runs a `DELETE`.
#[derive(Debug)]
pub struct Delete(SendRaw);

impl Delete {
    /// Builds a new `DELETE` coroutine. `if_match` carries the optional
    /// `If-Match` ETag (RFC 9110 §13.1.1).
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        if_match: Option<&str>,
    ) -> Self {
        let mut builder = WebdavRequest::delete(base_url, auth, user_agent, path);
        if let Some(etag) = if_match {
            builder = builder.if_match(etag);
        }
        let request = builder.body(Vec::new());
        Self(SendRaw::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}
