//! Generic `MOVE` coroutine (RFC 4918 §9.9).

use alloc::vec::Vec;

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    send::{SendRaw, SendResult},
};

/// Coroutine that runs a `MOVE` of `path` to `destination`.
#[derive(Debug)]
pub struct Move(SendRaw);

impl Move {
    /// Builds a new `MOVE` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        destination: &str,
        overwrite: bool,
    ) -> Self {
        let request = WebdavRequest::move_(base_url, auth, user_agent, path)
            .destination(destination)
            .overwrite(overwrite)
            .body(Vec::new());
        Self(SendRaw::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}
