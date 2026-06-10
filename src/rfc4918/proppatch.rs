//! Generic `PROPPATCH` coroutine (RFC 4918 §9.2).
//!
//! Sends a `PROPPATCH` against `path` with the caller-supplied XML
//! body. Most callers want the [`MkcolResponse`]-shaped body since
//! `PROPPATCH` returns a multistatus.
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
//!     rfc4918::{WebdavAuth, proppatch::Proppatch, send::Empty},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let body = b"<propertyupdate xmlns=\"DAV:\"></propertyupdate>".to_vec();
//! let mut coroutine = Proppatch::<Empty>::new(&base_url, &auth, "io-webdav", "/dav/", body);
//! let mut arg = None;
//!
//! let ok = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(ok)) => break ok,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("keep-alive: {}", ok.keep_alive);
//! ```

use core::fmt;

use alloc::vec::Vec;

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        request::WebdavRequest,
        send::{Send, SendError, SendOk},
        {MkcolResponse, WebdavAuth},
    },
};

/// Coroutine that runs a `PROPPATCH` and deserializes the response into
/// [`MkcolResponse<T>`].
#[derive(Debug)]
pub struct Proppatch<T: for<'a> Deserialize<'a>> {
    state: State<T>,
}

impl<T: for<'a> Deserialize<'a>> Proppatch<T> {
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
        Self {
            state: State::Send(Send::new(request)),
        }
    }
}

impl<T: for<'a> Deserialize<'a>> WebdavCoroutine for Proppatch<T> {
    type Yield = WebdavYield;
    type Return = Result<SendOk<MkcolResponse<T>>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("proppatch: {}", self.state);
        match &mut self.state {
            State::Send(send) => send.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State<T: for<'a> Deserialize<'a>> {
    Send(Send<MkcolResponse<T>>),
}

impl<T: for<'a> Deserialize<'a>> fmt::Display for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}
