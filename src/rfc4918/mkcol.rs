//! Generic extended `MKCOL` coroutine (RFC 4918 §9.3, RFC 5689 §3).
//!
//! Creates a collection at `path` whose `<resourcetype>` is
//! `<collection/>` plus `resource_types`, setting each `(property,
//! value)` pair. The request body is generated; the response is not
//! surfaced.
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
//!     rfc4918::{DISPLAYNAME, WebdavAuth, mkcol::Mkcol},
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
//!     Mkcol::new(&base_url, &auth, "io-webdav", "/dav/collection/", &[], &[(DISPLAYNAME, "New")]);
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
        Property, WebdavAuth, mkcol_body,
        request::WebdavRequest,
        send::{SendError, SendRaw},
    },
    webdav_try,
};

/// Coroutine that runs an extended `MKCOL`.
#[derive(Debug)]
pub struct Mkcol {
    state: State,
}

impl Mkcol {
    /// Builds a new `MKCOL` coroutine creating a collection at `path`
    /// with the given extra `resource_types` and property values.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        resource_types: &[Property],
        set: &[(Property, &str)],
    ) -> Self {
        let request = WebdavRequest::mkcol(base_url, auth, user_agent, path)
            .content_type_xml()
            .body(mkcol_body(resource_types, set));
        Self {
            state: State::Send(SendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for Mkcol {
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
