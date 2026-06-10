//! Generic `MKCOL` coroutine (RFC 4918 §9.3, RFC 5689 §3).
//!
//! Sends a `MKCOL` against `path` with the caller-supplied XML body
//! (extended `MKCOL` from RFC 5689) and ignores the response body.

use alloc::vec::Vec;

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    send::{Empty, Send, SendResult},
};

/// Coroutine that runs a `MKCOL`.
#[derive(Debug)]
pub struct Mkcol(Send<Empty>);

impl Mkcol {
    /// Builds a new `MKCOL` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        body: Vec<u8>,
    ) -> Self {
        let request = WebdavRequest::mkcol(base_url, auth, user_agent, path)
            .content_type_xml()
            .body(body);
        Self(Send::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Empty> {
        self.0.resume(arg)
    }
}
