//! Generic `REPORT` coroutine (RFC 3253 §3.6).
//!
//! Sends a `REPORT` against `path` with a caller-built query body (e.g.
//! a CalDAV `calendar-query` from
//! [`calendar_query_body`](crate::rfc4791::calendar::calendar_query_body))
//! and parses the response into a [`Multistatus`].
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
//!     rfc4791::calendar::calendar_query_body,
//!     rfc4918::{GETETAG, WebdavAuth, report::Report},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let body = calendar_query_body(&[GETETAG], "");
//! let mut coroutine =
//!     Report::new(&base_url, &auth, "io-webdav", "/dav/calendars/personal/", 1, body);
//! let mut arg = None;
//!
//! let multistatus = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(multistatus)) => break multistatus,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} entries", multistatus.responses.len());
//! ```

use alloc::{string::String, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        Multistatus, WebdavAuth, parse_multistatus,
        request::WebdavRequest,
        send::{SendError, SendRaw},
    },
    webdav_try,
};

/// Coroutine that runs a `REPORT` and parses the multistatus body.
#[derive(Debug)]
pub struct Report {
    state: State,
}

impl Report {
    /// Builds a new `REPORT` coroutine against `path` with the given
    /// `Depth` and query `body`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        depth: u8,
        body: Vec<u8>,
    ) -> Self {
        let request = WebdavRequest::report(base_url, auth, user_agent, path)
            .depth(depth)
            .content_type_xml()
            .body(body);
        Self {
            state: State::Send(SendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for Report {
    type Yield = WebdavYield;
    type Return = Result<Multistatus, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                let xml = String::from_utf8_lossy(&ok.body);
                WebdavCoroutineState::Complete(Ok(parse_multistatus(&xml)))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Send(SendRaw),
}
