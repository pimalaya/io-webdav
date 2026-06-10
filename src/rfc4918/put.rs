//! Generic `PUT` coroutine (RFC 4918 §9.7).
//!
//! Sends a `PUT` against `path` with the caller-supplied body bytes
//! and content type. Stays byte-oriented: callers parse iCal/vCard
//! upstream.
//!
//! Supports the optional `If-Match` (RFC 9110 §13.1.1) and
//! `If-None-Match` (RFC 9110 §13.1.2) preconditions so callers can
//! gate the write on a known ETag.
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
//!         put::{Put, PutArgs},
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
//! let mut coroutine = Put::new(PutArgs {
//!     base_url: &base_url,
//!     auth: &auth,
//!     user_agent: "io-webdav",
//!     path: "/dav/calendars/personal/event-1.ics",
//!     content_type: "text/calendar; charset=utf-8",
//!     body: b"BEGIN:VCALENDAR\r\n...\r\nEND:VCALENDAR\r\n".to_vec(),
//!     if_match: None,
//!     if_none_match: Some("*"),
//! });
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
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        request::WebdavRequest,
        send::{SendError, SendOk, SendRaw},
    },
};

/// Build inputs for a [`Put`] coroutine.
///
/// Uses a struct rather than positional arguments so callers can
/// build the request literal-style and skip the two optional
/// precondition fields without juggling positional `None`s.
#[derive(Clone, Debug)]
pub struct PutArgs<'a> {
    pub base_url: &'a Url,
    pub auth: &'a WebdavAuth,
    pub user_agent: &'a str,
    pub path: &'a str,
    pub content_type: &'a str,
    pub body: Vec<u8>,
    /// Optional `If-Match` ETag (RFC 9110 §13.1.1).
    pub if_match: Option<&'a str>,
    /// Optional `If-None-Match` ETag (RFC 9110 §13.1.2).
    pub if_none_match: Option<&'a str>,
}

/// Coroutine that runs a `PUT`.
#[derive(Debug)]
pub struct Put {
    state: State,
}

impl Put {
    /// Builds a new `PUT` coroutine.
    pub fn new(args: PutArgs<'_>) -> Self {
        let mut builder = WebdavRequest::put(args.base_url, args.auth, args.user_agent, args.path)
            .content_type(args.content_type);

        if let Some(etag) = args.if_match {
            builder = builder.if_match(etag);
        }

        if let Some(etag) = args.if_none_match {
            builder = builder.if_none_match(etag);
        }

        let request = builder.body(args.body);
        Self {
            state: State::Send(SendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for Put {
    type Yield = WebdavYield;
    type Return = Result<SendOk<Vec<u8>>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("put: {}", self.state);
        match &mut self.state {
            State::Send(send) => send.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    Send(SendRaw),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}
