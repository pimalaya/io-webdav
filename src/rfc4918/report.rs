//! Generic `REPORT` coroutine (RFC 3253 §3.6).
//!
//! Sends a `REPORT` against `path` with a caller-built query body (e.g. a
//! CalDAV `calendar-query` from
//! [`calendar_query_body`](crate::rfc4791::calendar::calendar_query_body)) and
//! parses the response into a [`WebdavMultistatus`].
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
//!     rfc4918::{GETETAG, WebdavAuth, report::WebdavReport},
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let body = calendar_query_body(&[GETETAG], "");
//! let mut coroutine =
//!     WebdavReport::new(&base_url, &auth, "io-webdav", "/dav/calendars/personal/", 1, body);
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

use log::{debug, trace};
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth, WebdavMultistatus, parse_multistatus,
        request::WebdavRequest,
        send::{WebdavSendError, WebdavSendRaw},
    },
};

/// Coroutine that runs a `REPORT` and parses the multistatus body.
#[derive(Debug)]
pub struct WebdavReport {
    state: State,
}

impl WebdavReport {
    /// Builds a new `REPORT` coroutine against `path` with the given `Depth`
    /// and query `body`.
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
            state: State::Send(WebdavSendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for WebdavReport {
    type Yield = WebdavYield;
    type Return = Result<WebdavMultistatus, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                let ok = match send.resume(arg) {
                    WebdavCoroutineState::Yielded(yielded) => {
                        return WebdavCoroutineState::Yielded(yielded);
                    }
                    WebdavCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(unsupported_report(err)));
                    }
                    WebdavCoroutineState::Complete(Ok(ok)) => ok,
                };

                let xml = String::from_utf8_lossy(&ok.body);
                trace!("received multistatus body {xml}");
                WebdavCoroutineState::Complete(Ok(parse_multistatus(&xml)))
            }
        }
    }
}

/// Turns a send failure saying the server does not implement the report into
/// [`WebdavSendError::UnsupportedReport`], and leaves every other one alone.
///
/// RFC 3253 §3.6 gives the answer a name: a server that cannot run a report
/// answers with the `DAV:supported-report` precondition, whatever status it
/// wraps it in. The precondition is therefore what is matched, a permission
/// `403` carrying none of it and surfacing as the refusal it is. `405` and
/// `501` are taken on the status alone, both meaning the request was never
/// going to run.
fn unsupported_report(err: WebdavSendError) -> WebdavSendError {
    let WebdavSendError::HttpStatus { status, body } = err else {
        return err;
    };

    if matches!(status, 405 | 501) || body.contains("supported-report") {
        debug!("WebDAV server does not implement the report");
        return WebdavSendError::UnsupportedReport { status, body };
    }

    WebdavSendError::HttpStatus { status, body }
}

#[derive(Debug)]
enum State {
    Send(WebdavSendRaw),
}
