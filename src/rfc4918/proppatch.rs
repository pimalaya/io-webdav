//! Generic `PROPPATCH` coroutine (RFC 4918 §9.2).
//!
//! Sets each `(property, value)` pair against `path` and removes each property
//! listed for removal; the request body is generated from the two lists. The
//! multistatus body is not surfaced.
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
//! // Ready stream, already connected and TLS-negotiated
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
//!         WebdavCoroutineState::Complete(Ok(out)) => {
//!             // Each refused property is listed under `failures`, and
//!             // `requested` is what the request asked to change.
//!             println!("{:?}", out.multistatus.responses);
//!             break;
//!         }
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use core::mem;

use alloc::{string::String, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth, WebdavMultistatus, WebdavPropValue, WebdavProperty, parse_multistatus,
        proppatch_body,
        request::WebdavRequest,
        send::{WebdavSendError, WebdavSendRaw},
    },
    webdav_try,
};

/// Outcome of a successful [`WebdavProppatch`] resume.
#[derive(Clone, Debug, Default)]
pub struct WebdavProppatchOk {
    /// The parsed multistatus.
    pub multistatus: WebdavMultistatus,
    /// Local names of the properties the request asked to set or remove.
    ///
    /// RFC 4918 §9.2.1 wants a propstat for each of them, so a name missing
    /// from the response is a property the server said nothing about, having
    /// changed nothing.
    pub requested: Vec<&'static str>,
}

/// Coroutine that runs a `PROPPATCH`.
#[derive(Debug)]
pub struct WebdavProppatch {
    requested: Vec<&'static str>,
    state: State,
}

impl WebdavProppatch {
    /// Builds a new `PROPPATCH` coroutine setting each `(property, value)` pair
    /// against `path` and removing each property in `remove`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        set: &[(WebdavProperty, WebdavPropValue<'_>)],
        remove: &[WebdavProperty],
    ) -> Self {
        let requested = set
            .iter()
            .map(|(prop, _)| prop.local)
            .chain(remove.iter().map(|prop| prop.local))
            .collect();
        let request = WebdavRequest::proppatch(base_url, auth, user_agent, path)
            .content_type_xml()
            .body(proppatch_body(set, remove));
        Self {
            requested,
            state: State::Send(WebdavSendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for WebdavProppatch {
    type Yield = WebdavYield;
    type Return = Result<WebdavProppatchOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                let xml = String::from_utf8_lossy(&ok.body);
                trace!("response body: {xml}");
                WebdavCoroutineState::Complete(Ok(WebdavProppatchOk {
                    multistatus: parse_multistatus(&xml),
                    requested: mem::take(&mut self.requested),
                }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Send(WebdavSendRaw),
}
