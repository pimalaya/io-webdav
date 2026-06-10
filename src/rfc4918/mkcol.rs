//! Generic `MKCOL` coroutine (RFC 4918 §9.3, RFC 5689 §3).
//!
//! Sends a `MKCOL` against `path` with the caller-supplied XML body
//! (extended `MKCOL` from RFC 5689) and ignores the response body.
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
//!     rfc4918::{WebdavAuth, mkcol::Mkcol},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let body = b"<mkcol xmlns=\"DAV:\"></mkcol>".to_vec();
//! let mut coroutine = Mkcol::new(&base_url, &auth, "io-webdav", "/dav/collection/", body);
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

use core::fmt;

use alloc::vec::Vec;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        request::WebdavRequest,
        send::{Empty, Send, SendError, SendOk},
    },
};

/// Coroutine that runs a `MKCOL`.
#[derive(Debug)]
pub struct Mkcol {
    state: State,
}

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
        Self {
            state: State::Send(Send::new(request)),
        }
    }
}

impl WebdavCoroutine for Mkcol {
    type Yield = WebdavYield;
    type Return = Result<SendOk<Empty>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("mkcol: {}", self.state);
        match &mut self.state {
            State::Send(send) => send.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    Send(Send<Empty>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}
