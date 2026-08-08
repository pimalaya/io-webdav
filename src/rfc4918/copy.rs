//! Generic `COPY` coroutine (RFC 4918 §9.8).
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
//!     rfc4918::{WebdavAuth, copy::WebdavCopy},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = WebdavCopy::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/event-1.ics",
//!     "/dav/calendars/work/event-1.ics",
//!     false,
//!     0,
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

/// Coroutine that runs a `COPY` of `path` to `destination`.
#[derive(Debug)]
pub struct WebdavCopy {
    state: State,
}

impl WebdavCopy {
    /// Builds a new `COPY` coroutine. `depth` is the `Depth` header
    /// (typically `0` for resources, `infinity` is encoded by the
    /// server, expose only the `0` / `1` case here).
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        destination: &str,
        overwrite: bool,
        depth: u8,
    ) -> Self {
        let request = WebdavRequest::copy(base_url, auth, user_agent, path)
            .destination(destination)
            .overwrite(overwrite)
            .depth(depth)
            .body(Vec::new());
        Self {
            state: State::Send(WebdavSendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for WebdavCopy {
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
