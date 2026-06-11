//! `.well-known/caldav` and `.well-known/carddav` discovery (RFC 6764).
//!
//! Sends a single request against `/.well-known/caldav` (or
//! `/.well-known/carddav`) and surfaces the redirect target so the
//! caller can rebuild its [`crate::client::WebdavClientStd`] against
//! the actual server root.
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
//!     rfc4918::WebdavAuth,
//!     rfc6764::well_known::{WellKnown, WellKnownKind},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = WellKnown::new(&base_url, &auth, "io-webdav", WellKnownKind::Caldav);
//! let mut arg = None;
//!
//! let out = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(out)) => break out,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("CalDAV root: {}", out.url);
//! ```

use alloc::{
    format,
    string::{String, ToString},
};

use io_http::{
    coroutine::*,
    rfc9110::{
        request::HttpRequest,
        response::HttpResponse,
        send::{HttpSendOutput, HttpSendYield},
    },
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::trace;
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{WebdavAuth, emit_header},
};

/// Which RFC 6764 service to discover.
#[derive(Clone, Copy, Debug)]
pub enum WellKnownKind {
    /// CalDAV: `/.well-known/caldav` (RFC 6764 §5).
    Caldav,
    /// CardDAV: `/.well-known/carddav` (RFC 6764 §5).
    Carddav,
}

impl WellKnownKind {
    /// Returns the well-known path including the leading slash.
    pub fn path(self) -> &'static str {
        match self {
            Self::Caldav => "/.well-known/caldav",
            Self::Carddav => "/.well-known/carddav",
        }
    }
}

/// Failure causes during well-known discovery.
#[derive(Debug, Error)]
pub enum WellKnownError {
    #[error("Expected a 3xx redirection from .well-known, got HTTP {0}: {1}")]
    NotRedirected(u16, String),
    #[error("Missing Location header in .well-known response")]
    MissingLocationHeader,
    #[error("Invalid Location header URL `{0}`: {1}")]
    InvalidLocationUrl(String, #[source] url::ParseError),

    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// Successful terminal output of [`WellKnown`]: the redirect target.
#[derive(Clone, Debug)]
pub struct WellKnownOutput {
    pub url: Url,
    pub keep_alive: bool,
}

/// I/O-free well-known discovery coroutine.
#[derive(Debug)]
pub struct WellKnown {
    state: State,
}

impl WellKnown {
    /// Builds a new well-known discovery coroutine against
    /// `base_url`'s authority.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, kind: WellKnownKind) -> Self {
        let mut url = base_url.clone();
        url.set_path(kind.path());

        trace!("discover {kind:?} via {url}");

        let host = match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            (None, _) => String::new(),
        };

        let mut request = HttpRequest::get(url).header("User-Agent", user_agent);
        if !host.is_empty() {
            request = request.header("Host", host);
        }
        if let Some(value) = emit_header(auth) {
            request = request.header("Authorization", value);
        }

        Self {
            state: State::Send(Http11Send::new(request)),
        }
    }
}

impl WebdavCoroutine for WellKnown {
    type Yield = WebdavYield;
    type Return = Result<WellKnownOutput, WellKnownError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => match send.resume(arg) {
                HttpCoroutineState::Yielded(HttpSendYield::WantsRead) => {
                    WebdavCoroutineState::Yielded(WebdavYield::WantsRead)
                }
                HttpCoroutineState::Yielded(HttpSendYield::WantsWrite(bytes)) => {
                    WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes))
                }
                HttpCoroutineState::Yielded(HttpSendYield::WantsRedirect {
                    url,
                    keep_alive,
                    ..
                }) => WebdavCoroutineState::Complete(Ok(WellKnownOutput { url, keep_alive })),
                HttpCoroutineState::Complete(Err(err)) => {
                    WebdavCoroutineState::Complete(Err(err.into()))
                }
                HttpCoroutineState::Complete(Ok(HttpSendOutput {
                    response,
                    keep_alive,
                    ..
                })) => match read_location(&response) {
                    Ok(url) => {
                        WebdavCoroutineState::Complete(Ok(WellKnownOutput { url, keep_alive }))
                    }
                    Err(err) => WebdavCoroutineState::Complete(Err(err)),
                },
            },
        }
    }
}

fn read_location(response: &HttpResponse) -> Result<Url, WellKnownError> {
    let code = *response.status;
    if !(300..400).contains(&code) {
        let body = String::from_utf8_lossy(&response.body).into_owned();
        return Err(WellKnownError::NotRedirected(code, body));
    }

    let location = response
        .header("location")
        .ok_or(WellKnownError::MissingLocationHeader)?;

    Url::parse(location).map_err(|err| WellKnownError::InvalidLocationUrl(location.into(), err))
}

#[derive(Debug)]
enum State {
    Send(Http11Send),
}
