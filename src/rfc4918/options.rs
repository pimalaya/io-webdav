//! Generic `OPTIONS` coroutine (RFC 4918 §9.1, §15).
//!
//! Sends an `OPTIONS` against `path` and returns the raw response so
//! the caller can inspect the `DAV` and `Allow` headers. A typed
//! capability enum lives at the client level (TBD per the plan's open
//! questions).

use alloc::vec::Vec;

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    send::{SendRaw, SendResult},
};

/// Coroutine that runs an `OPTIONS`.
#[derive(Debug)]
pub struct Options(SendRaw);

impl Options {
    /// Builds a new `OPTIONS` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        let request = WebdavRequest::options(base_url, auth, user_agent, path).body(Vec::new());
        Self(SendRaw::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}
