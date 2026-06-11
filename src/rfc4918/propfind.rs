//! Generic `PROPFIND` coroutine (RFC 4918 §9.1).
//!
//! Requests `props` against `path` at the given `Depth`; the request
//! body is generated from the selector and the response is parsed into
//! a [`Multistatus`].
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
//!     rfc4918::{DISPLAYNAME, RESOURCETYPE, WebdavAuth, propfind::Propfind},
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
//!     Propfind::new(&base_url, &auth, "io-webdav", "/dav/", 1, &[RESOURCETYPE, DISPLAYNAME]);
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
//! for entry in &multistatus.responses {
//!     println!("{}: {:?}", entry.href, entry.text(DISPLAYNAME));
//! }
//! ```

use alloc::string::String;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        parse_multistatus, propfind_body,
        request::WebdavRequest,
        send::{SendError, SendRaw},
        types::{Multistatus, Property, WebdavAuth},
    },
    webdav_try,
};

/// Coroutine that runs a `PROPFIND` and parses the multistatus body.
#[derive(Debug)]
pub struct Propfind {
    state: State,
}

impl Propfind {
    /// Builds a new `PROPFIND` coroutine requesting `props` against
    /// `path` (relative to `base_url`) with the given `depth`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        depth: u8,
        props: &[Property],
    ) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, path)
            .depth(depth)
            .content_type_xml()
            .body(propfind_body(props));
        Self {
            state: State::Send(SendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for Propfind {
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
