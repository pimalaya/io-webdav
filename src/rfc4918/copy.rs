//! Generic `COPY` coroutine (RFC 4918 §9.8).

use alloc::vec::Vec;

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    send::{SendRaw, SendResult},
};

/// Coroutine that runs a `COPY` of `path` to `destination`.
#[derive(Debug)]
pub struct Copy(SendRaw);

impl Copy {
    /// Builds a new `COPY` coroutine. `depth` is the `Depth` header
    /// (typically `0` for resources, `infinity` is encoded by the
    /// server, expose only the `0` / `1` case here).
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        destination: &str,
        overwrite: bool,
        depth: u8,
    ) -> Self {
        let request = WebdavRequest::copy(base_url, auth, user_agent, path)
            .destination(destination)
            .overwrite(overwrite)
            .depth(depth)
            .body(Vec::new());
        Self(SendRaw::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}
