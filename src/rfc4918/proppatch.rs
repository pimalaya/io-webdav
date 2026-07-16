//! Generic `PROPPATCH` coroutine (RFC 4918 §9.2).
//!
//! Sets each `(property, value)` pair against `path`; the request body
//! is generated from the pairs. The multistatus body is not surfaced.
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
//!     rfc4918::{DISPLAYNAME, WebdavAuth, proppatch::Proppatch},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine =
//!     Proppatch::new(&base_url, &auth, "io-webdav", "/dav/collection/", &[(DISPLAYNAME, "Renamed")]);
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
//!         WebdavCoroutineState::Complete(Ok(())) => break,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        Property, WebdavAuth, proppatch_body,
        request::WebdavRequest,
        send::{SendError, SendRaw},
    },
    webdav_try,
};

/// Coroutine that runs a `PROPPATCH`.
#[derive(Debug)]
pub struct Proppatch {
    state: State,
}

impl Proppatch {
    /// Builds a new `PROPPATCH` coroutine setting each `(property,
    /// value)` pair against `path`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        set: &[(Property, &str)],
    ) -> Self {
        let request = WebdavRequest::proppatch(base_url, auth, user_agent, path)
            .content_type_xml()
            .body(proppatch_body(set));
        Self {
            state: State::Send(SendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for Proppatch {
    type Yield = WebdavYield;
    type Return = Result<(), SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                webdav_try!(send, arg);
                WebdavCoroutineState::Complete(Ok(()))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Send(SendRaw),
}
