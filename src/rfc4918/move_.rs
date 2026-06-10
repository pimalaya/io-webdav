//! Generic `MOVE` coroutine (RFC 4918 §9.9).
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
//!     rfc4918::{WebdavAuth, move_::Move},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = Move::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/event-1.ics",
//!     "/dav/calendars/work/event-1.ics",
//!     false,
//! );
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
        send::{SendError, SendOk, SendRaw},
    },
};

/// Coroutine that runs a `MOVE` of `path` to `destination`.
#[derive(Debug)]
pub struct Move {
    state: State,
}

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
        Self {
            state: State::Send(SendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for Move {
    type Yield = WebdavYield;
    type Return = Result<SendOk<Vec<u8>>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("move: {}", self.state);
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
