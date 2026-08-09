//! Generic `PROPPATCH` coroutine (RFC 4918 §9.2).
//!
//! Sets each `(property, value)` pair against `path` and removes each
//! property listed for removal; the request body is generated from the
//! two lists. The multistatus body is not surfaced.
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
//!     rfc4918::{DISPLAYNAME, WebdavPropValue, WebdavAuth, proppatch::WebdavProppatch},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let set = [(DISPLAYNAME, WebdavPropValue::Text("Renamed"))];
//! let mut coroutine =
//!     WebdavProppatch::new(&base_url, &auth, "io-webdav", "/dav/collection/", &set, &[]);
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
        WebdavAuth, WebdavPropValue, WebdavProperty, proppatch_body,
        request::WebdavRequest,
        send::{WebdavSendError, WebdavSendRaw},
    },
    webdav_try,
};

/// Coroutine that runs a `PROPPATCH`.
#[derive(Debug)]
pub struct WebdavProppatch {
    state: State,
}

impl WebdavProppatch {
    /// Builds a new `PROPPATCH` coroutine setting each `(property,
    /// value)` pair against `path` and removing each property in
    /// `remove`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        set: &[(WebdavProperty, WebdavPropValue<'_>)],
        remove: &[WebdavProperty],
    ) -> Self {
        let request = WebdavRequest::proppatch(base_url, auth, user_agent, path)
            .content_type_xml()
            .body(proppatch_body(set, remove));
        Self {
            state: State::Send(WebdavSendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for WebdavProppatch {
    type Yield = WebdavYield;
    type Return = Result<(), WebdavSendError>;

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
    Send(WebdavSendRaw),
}
