//! Base coroutine every higher-level WebDAV coroutine delegates to:
//! runs an HTTP/1.1 exchange and adds quick-xml deserialization of the
//! response body into the caller-chosen `T`. Use [`SendRaw`] when the
//! body should stay as raw bytes (e.g. `GET` / `PUT` of an iCal/vCard
//! resource).
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
//!     rfc4918::{
//!         WebdavAuth,
//!         request::WebdavRequest,
//!         send::{Empty, Send},
//!     },
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let request = WebdavRequest::propfind(&base_url, &auth, "io-webdav", "/dav/")
//!     .content_type_xml()
//!     .body(Vec::new());
//! let mut coroutine = Send::<Empty>::new(request);
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

use core::{fmt, marker::PhantomData};

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
use serde::{Deserialize, Deserializer};
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
    #[error("Parse WebDAV XML response body error: {0}")]
    ParseXmlResponseBody(#[source] quick_xml::DeError),
    #[error("WebDAV server returned unexpected redirect")]
    UnexpectedRedirect,

    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// I/O-free coroutine that sends a WebDAV request and deserializes the
/// response body as XML into `T`.
#[derive(Debug)]
pub struct Send<T: for<'a> Deserialize<'a>> {
    phantom: PhantomData<T>,
    state: State,
}

impl<T: for<'a> Deserialize<'a>> Send<T> {
    /// Builds a new `Send` coroutine. `request` must already carry its
    /// body bytes (via [`crate::rfc4918::request::WebdavRequest::body`]).
    pub fn new(request: HttpRequest) -> Self {
        trace!("send WebDAV request to {}", request.url);

        Self {
            phantom: PhantomData,
            state: State::Send(Http11Send::new(request)),
        }
    }
}

impl<T: for<'a> Deserialize<'a>> WebdavCoroutine for Send<T> {
    type Yield = WebdavYield;
    type Return = Result<SendOk<T>, SendError>;

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

                let body = String::from_utf8_lossy(&response.body);
                trace!("WebDAV response body: {body}");

                if !response.status.is_success() {
                    let err = SendError::HttpStatus(*response.status, body.into_owned());
                    return WebdavCoroutineState::Complete(Err(err));
                }

                let parsed = match quick_xml::de::from_str::<T>(&body) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        let err = SendError::ParseXmlResponseBody(err);
                        return WebdavCoroutineState::Complete(Err(err));
                    }
                };

                WebdavCoroutineState::Complete(Ok(SendOk {
                    response,
                    keep_alive,
                    body: parsed,
                }))
            }
        }
    }
}

/// I/O-free coroutine that sends a WebDAV request and returns the
/// response body as raw bytes (no XML parsing).
///
/// Used by `GET` / `PUT` / `DELETE` against an iCal/vCard resource:
/// io-webdav stays byte-oriented and lets callers run calcard.
#[derive(Debug)]
pub struct SendRaw {
    state: State,
}

impl SendRaw {
    /// Builds a new `SendRaw` coroutine. `request` must already carry
    /// its body bytes.
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
        trace!("send raw: {}", self.state);
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

/// Marker for coroutines that ignore the response body. Implements
/// [`serde::Deserialize`] so it can stand in for `T` in [`Send<T>`].
#[derive(Debug)]
pub struct Empty;

impl<'de> Deserialize<'de> for Empty {
    fn deserialize<D: Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Ok(Empty)
    }
}
