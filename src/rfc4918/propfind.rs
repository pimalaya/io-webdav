//! Generic `PROPFIND` coroutine (RFC 4918 §9.1).
//!
//! Sends a `PROPFIND` against `path` with the caller-supplied XML body
//! and parses the multistatus body into [`Multistatus<T>`].

use alloc::vec::Vec;

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    response::Multistatus,
    send::{Send, SendResult},
};

/// Coroutine that runs a `PROPFIND` and deserializes the response into
/// `Multistatus<T>`.
#[derive(Debug)]
pub struct Propfind<T: for<'a> serde::Deserialize<'a>>(Send<Multistatus<T>>);

impl<T: for<'a> serde::Deserialize<'a>> Propfind<T> {
    /// Builds a new `PROPFIND` coroutine targeting `path` (relative to
    /// `base_url`), with the given `depth` header and XML request body.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        depth: u8,
        body: Vec<u8>,
    ) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, path)
            .depth(depth)
            .content_type_xml()
            .body(body);
        Self(Send::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Multistatus<T>> {
        self.0.resume(arg)
    }
}
