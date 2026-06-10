//! Generic `PROPPATCH` coroutine (RFC 4918 §9.2).
//!
//! Sends a `PROPPATCH` against `path` with the caller-supplied XML
//! body. Most callers want the [`MkcolResponse`]-shaped body since
//! `PROPPATCH` returns a multistatus.

use alloc::vec::Vec;

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    response::MkcolResponse,
    send::{Send, SendResult},
};

/// Coroutine that runs a `PROPPATCH` and deserializes the response
/// into [`MkcolResponse<T>`].
#[derive(Debug)]
pub struct Proppatch<T: for<'a> serde::Deserialize<'a>>(Send<MkcolResponse<T>>);

impl<T: for<'a> serde::Deserialize<'a>> Proppatch<T> {
    /// Builds a new `PROPPATCH` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        body: Vec<u8>,
    ) -> Self {
        let request = WebdavRequest::proppatch(base_url, auth, user_agent, path)
            .content_type_xml()
            .body(body);
        Self(Send::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<MkcolResponse<T>> {
        self.0.resume(arg)
    }
}
