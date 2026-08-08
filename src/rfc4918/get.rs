//! Generic `GET` coroutine (RFC 9110 §9.3.1).
//!
//! Sends a `GET` against `path` and returns the response body as raw
//! bytes. iCalendar and vCard parsing happens upstream, in ical and
//! vcard.
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
//!     rfc4918::{WebdavAuth, get::WebdavGet},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = WebdavGet::new(&base_url, &auth, "io-webdav", "/dav/calendars/personal/event-1.ics");
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

/// Coroutine that runs a `GET`.
#[derive(Debug)]
pub struct WebdavGet {
    state: State,
}

impl WebdavGet {
    /// Builds a new `GET` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, path: &str) -> Self {
        let request = WebdavRequest::get(base_url, auth, user_agent, path).body(Vec::new());
        Self {
            state: State::Send(WebdavSendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for WebdavGet {
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
