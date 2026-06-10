//! Base coroutine every higher-level WebDAV coroutine delegates to:
//! runs an HTTP/1.1 exchange and returns the raw response body. Higher
//! layers parse the multistatus with
//! [`parse_multistatus`](crate::rfc4918::parse_multistatus) or keep the
//! bytes as-is (`GET` / `PUT` of an iCal/vCard resource).
//!
//! All I/O is hoisted: the coroutine yields [`WebdavYield`] and the
//! caller owns the stream work. 3xx redirects surface as
//! [`SendError::UnexpectedRedirect`]; redirect-aware coroutines use
//! [`crate::rfc4918::follow_redirects`] instead.
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
//!     rfc4918::{WebdavAuth, request::WebdavRequest, send::SendRaw},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let request = WebdavRequest::get(&base_url, &auth, "io-webdav", "/dav/file.txt").body(Vec::new());
//! let mut coroutine = SendRaw::new(request);
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
//! println!("{} bytes, keep-alive: {}", ok.body.len(), ok.keep_alive);
//! ```

use core::fmt;

use alloc::{string::String, vec::Vec};

use io_http::{
    coroutine::*,
    rfc9110::{
        request::HttpRequest,
        response::HttpResponse,
        send::{HttpSendOutput, HttpSendYield},
    },
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::trace;
use thiserror::Error;

use crate::coroutine::*;

/// Successful terminal output of a WebDAV send coroutine.
#[derive(Debug)]
pub struct SendOk<T> {
    pub response: HttpResponse,
    pub keep_alive: bool,
    pub body: T,
}

/// Failure causes during a WebDAV send.
#[derive(Debug, Error)]
pub enum SendError {
    #[error("WebDAV server returned HTTP {0}: {1}")]
    HttpStatus(u16, String),
    #[error("WebDAV server returned unexpected redirect")]
    UnexpectedRedirect,

    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// I/O-free coroutine that sends a WebDAV request and returns the
/// response body as raw bytes.
#[derive(Debug)]
pub struct SendRaw {
    state: State,
}

impl SendRaw {
    /// Builds a new `SendRaw` coroutine. `request` must already carry
    /// its body bytes (via [`crate::rfc4918::request::WebdavRequest::body`]).
    pub fn new(request: HttpRequest) -> Self {
        trace!("send WebDAV request to {}", request.url);

        Self {
            state: State::Send(Http11Send::new(request)),
        }
    }
}

impl WebdavCoroutine for SendRaw {
    type Yield = WebdavYield;
    type Return = Result<SendOk<Vec<u8>>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("send: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let out = match send.resume(arg) {
                    HttpCoroutineState::Yielded(HttpSendYield::WantsRead) => {
                        return WebdavCoroutineState::Yielded(WebdavYield::WantsRead);
                    }
                    HttpCoroutineState::Yielded(HttpSendYield::WantsWrite(bytes)) => {
                        return WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes));
                    }
                    HttpCoroutineState::Yielded(HttpSendYield::WantsRedirect { .. }) => {
                        return WebdavCoroutineState::Complete(Err(SendError::UnexpectedRedirect));
                    }
                    HttpCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(err.into()));
                    }
                    HttpCoroutineState::Complete(Ok(out)) => out,
                };

                let HttpSendOutput {
                    response,
                    keep_alive,
                    ..
                } = out;

                if !response.status.is_success() {
                    let body = String::from_utf8_lossy(&response.body).into_owned();
                    let err = SendError::HttpStatus(*response.status, body);
                    return WebdavCoroutineState::Complete(Err(err));
                }

                let body = response.body.clone();
                WebdavCoroutineState::Complete(Ok(SendOk {
                    response,
                    keep_alive,
                    body,
                }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Send(Http11Send),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}
