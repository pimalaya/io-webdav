//! Generic `GET` coroutine (RFC 9110 §9.3.1).
//!
//! Sends a `GET` against `path` and returns the response body as raw
//! bytes. iCal/vCard parsing happens upstream in
//! io-calendar/io-addressbook.
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
//!     rfc4918::{WebdavAuth, get::Get},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = Get::new(&base_url, &auth, "io-webdav", "/dav/calendars/personal/event-1.ics");
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
//! println!("{} bytes", ok.body.len());
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

/// Coroutine that runs a `GET`.
#[derive(Debug)]
pub struct Get {
    state: State,
}

impl Get {
    /// Builds a new `GET` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        let request = WebdavRequest::get(base_url, auth, user_agent, path).body(Vec::new());
        Self {
            state: State::Send(SendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for Get {
    type Yield = WebdavYield;
    type Return = Result<SendOk<Vec<u8>>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("get: {}", self.state);
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
