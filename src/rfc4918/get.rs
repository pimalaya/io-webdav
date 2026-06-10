//! Generic `GET` coroutine (RFC 9110 §9.3.1).
//!
//! Sends a `GET` against `path` and returns the response body as raw
//! bytes. iCal/vCard parsing happens upstream in
//! io-calendar/io-addressbook.

use alloc::vec::Vec;

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    send::{SendRaw, SendResult},
};

/// Coroutine that runs a `GET`.
#[derive(Debug)]
pub struct Get(SendRaw);

impl Get {
    /// Builds a new `GET` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        let request = WebdavRequest::get(base_url, auth, user_agent, path).body(Vec::new());
        Self(SendRaw::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}
