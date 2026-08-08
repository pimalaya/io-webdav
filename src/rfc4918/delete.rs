//! Generic `DELETE` coroutine (RFC 4918 §9.6).
//!
//! Sends a `DELETE` against `path`. Servers may return 204 No Content
//! (empty body) or a multistatus when the deletion partially failed;
//! callers inspect the response status to disambiguate.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_webdav::{
//!     coroutine::{WebdavCoroutine, WebdavCoroutineState, WebdavYield},
//!     rfc4918::{WebdavAuth, delete::WebdavDelete},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = WebdavDelete::new(&base_url, &auth, "io-webdav", "/dav/collection/", None);
//! let mut arg = None;
//!
//! loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(_)) => break,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use alloc::vec::Vec;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        request::WebdavRequest,
        send::{WebdavSendError, WebdavSendOk, WebdavSendRaw},
    },
};

/// Coroutine that runs a `DELETE`.
#[derive(Debug)]
pub struct WebdavDelete {
    state: State,
}

impl WebdavDelete {
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
        Self {
            state: State::Send(WebdavSendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for WebdavDelete {
    type Yield = WebdavYield;
    type Return = Result<WebdavSendOk<Vec<u8>>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => send.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    Send(WebdavSendRaw),
}
