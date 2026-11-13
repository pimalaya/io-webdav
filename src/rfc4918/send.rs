//! # Send
//!
//! Base coroutine every higher-level WebDAV coroutine delegates to: runs an
//! HTTP/1.1 exchange and returns the raw response body, which higher layers
//! either parse as a multistatus or keep as-is.
//!
//! All I/O is hoisted: the coroutine yields [`WebdavYield`] and the caller owns
//! the stream work. A 3xx surfaces as [`WebdavSendError::UnexpectedRedirect`];
//! redirect-aware coroutines use [`crate::rfc4918::follow_redirects`] instead.
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
//!     rfc4918::{WebdavAuth, request::WebdavRequest, send::WebdavSendRaw},
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let request = WebdavRequest::get(&base_url, &auth, "io-webdav", "/dav/file.txt").body(Vec::new());
//! let mut coroutine = WebdavSendRaw::new(request);
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
//! println!("{} bytes, keep-alive: {}", ok.body.len(), ok.keep_alive);
//! ```

use core::mem;

use alloc::{
    string::{String, ToString},
    vec::Vec,
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
    rfc4918::{
        WebdavMemberFailure, WebdavMultistatusParseError, body_text, parse_multistatus, summarized,
    },
};

/// Successful terminal output of a WebDAV send coroutine.
#[derive(Debug)]
pub struct WebdavSendOk<T> {
    /// The HTTP response head (status line and headers), its body moved to
    /// [`body`](Self::body).
    pub response: HttpResponse,
    /// Whether the server allows reusing the connection.
    pub keep_alive: bool,
    /// The coroutine-specific parsed body.
    pub body: T,
}

/// Failure causes during a WebDAV send.
#[derive(Debug, Error)]
pub enum WebdavSendError {
    /// The server returned a non-2xx HTTP status. The body is kept verbatim for
    /// callers inspecting it, but renders as a
    /// [summary](crate::rfc4918::summarize_body).
    #[error("WebDAV server returned HTTP {status}{}", summarized(body))]
    HttpStatus {
        /// The non-2xx status the server answered with.
        status: u16,
        /// The response body, verbatim.
        body: String,
    },
    /// The server does not implement the requested report, as opposed to
    /// refusing this particular request. Raised by
    /// [`WebdavReport`](crate::rfc4918::report::WebdavReport) alone, a report
    /// being the only request that can be unimplemented while the resource
    /// exists; the consumer enumerates another way rather than giving up.
    #[error(
        "WebDAV server does not implement the report (HTTP {status}){}",
        summarized(body)
    )]
    UnsupportedReport {
        /// The status the server wrapped the refusal in.
        status: u16,
        /// The response body, verbatim.
        body: String,
    },
    /// The collection already holds a resource carrying the `UID` of the
    /// written one, which RFC 4791 §5.3.2 and RFC 6352 §6.3.2 both forbid and
    /// both name: `CALDAV:no-uid-conflict` and `CARDDAV:no-uid-conflict`.
    /// Raised by the item and card create and update coroutines, the writes
    /// that carry a `UID`; one variant serves both flavours, the caller
    /// knowing which one it called. The consumer fixes the source, this layer
    /// neither retries nor renames.
    #[error(
        "WebDAV collection already holds a resource with the same UID (HTTP {status}){}",
        summarized(body)
    )]
    DuplicateUid {
        /// The status the server wrapped the refusal in.
        status: u16,
        /// The response body, verbatim.
        body: String,
    },
    /// The server cannot evaluate the query filter: a property, parameter or
    /// match type it does not support (`supported-filter`), or a collation it
    /// does not know (`supported-collation`), both RFC 6352 §8.6. Raised by
    /// the card list coroutine, the one sending a caller's filter.
    #[error(
        "WebDAV server does not support the query filter (HTTP {status}){}",
        summarized(body)
    )]
    UnsupportedFilter {
        /// The status the server wrapped the refusal in.
        status: u16,
        /// The response body, verbatim.
        body: String,
    },
    /// The server returned a redirect where none was expected.
    #[error("WebDAV server returned unexpected redirect to {url}")]
    UnexpectedRedirect {
        /// Resolved redirect target, from the `Location` header.
        url: Url,
        /// Whether the server will keep the connection open.
        keep_alive: bool,
        /// Whether the redirect stays on the same scheme, host and port.
        same_origin: bool,
    },
    /// The server answered a `DELETE`, `COPY` or `MOVE` with a 207 naming the
    /// members it failed on (RFC 4918 §9.6.1, §9.8.5, §9.9.4).
    #[error("WebDAV server failed on {}", joined(.0))]
    MemberFailures(Vec<WebdavMemberFailure>),
    /// The value of the named request header carries a control character,
    /// which could end its line; nothing was sent.
    #[error("WebDAV request header {0} carries a control character")]
    InvalidHeader(String),
    /// The multistatus body could not be parsed.
    #[error(transparent)]
    Multistatus(#[from] WebdavMultistatusParseError),
    /// The underlying HTTP/1.1 send failed.
    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// I/O-free coroutine sending a WebDAV request and returning the response body
/// as raw bytes.
#[derive(Debug)]
pub struct WebdavSendRaw {
    state: State,
}

impl WebdavSendRaw {
    /// Builds a new coroutine from a request already carrying its body bytes
    /// (see [`request`](crate::rfc4918::request)).
    ///
    /// A request whose header value carries a control character is never
    /// sent: the coroutine completes with [`WebdavSendError::InvalidHeader`].
    pub fn new(request: HttpRequest) -> Self {
        let state = match invalid_header(&request) {
            Some(name) => State::Reject(name.to_string()),
            None => State::Send(Http11Send::new(request)),
        };

        Self { state }
    }
}

impl WebdavCoroutine for WebdavSendRaw {
    type Yield = WebdavYield;
    type Return = Result<WebdavSendOk<Vec<u8>>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                let out = match send.resume(arg) {
                    HttpCoroutineState::Yielded(HttpSendYield::WantsRead) => {
                        return WebdavCoroutineState::Yielded(WebdavYield::WantsRead);
                    }
                    HttpCoroutineState::Yielded(HttpSendYield::WantsWrite(bytes)) => {
                        return WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes));
                    }
                    HttpCoroutineState::Yielded(HttpSendYield::WantsRedirect {
                        url,
                        keep_alive,
                        same_origin,
                        ..
                    }) => {
                        let err = WebdavSendError::UnexpectedRedirect {
                            url,
                            keep_alive,
                            same_origin,
                        };
                        return WebdavCoroutineState::Complete(Err(err));
                    }
                    HttpCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(err.into()));
                    }
                    HttpCoroutineState::Complete(Ok(out)) => out,
                };

                let HttpSendOutput {
                    mut response,
                    keep_alive,
                    ..
                } = out;

                if !response.status.is_success() {
                    let body = String::from_utf8_lossy(&response.body).into_owned();
                    let err = WebdavSendError::HttpStatus {
                        status: *response.status,
                        body,
                    };
                    return WebdavCoroutineState::Complete(Err(err));
                }

                let body = mem::take(&mut response.body);
                WebdavCoroutineState::Complete(Ok(WebdavSendOk {
                    response,
                    keep_alive,
                    body,
                }))
            }
            State::Reject(name) => {
                let err = WebdavSendError::InvalidHeader(mem::take(name));
                WebdavCoroutineState::Complete(Err(err))
            }
        }
    }
}

/// Returns the name of the first header of `request` whose value carries a
/// control character (a byte below 0x20, or 0x7F).
pub(crate) fn invalid_header(request: &HttpRequest) -> Option<&str> {
    let control = |byte: u8| byte < 0x20 || byte == 0x7f;

    request
        .headers
        .iter()
        .find(|(_, value)| value.bytes().any(control))
        .map(|(name, _)| name.as_str())
}

/// Fails a `DELETE`, `COPY` or `MOVE` answered 207, unless every row of it
/// carries a 2xx status.
pub(crate) fn check_members(
    ok: WebdavSendOk<Vec<u8>>,
) -> Result<WebdavSendOk<Vec<u8>>, WebdavSendError> {
    if *ok.response.status != 207 {
        return Ok(ok);
    }

    let failures: Vec<WebdavMemberFailure> = parse_multistatus(&body_text(&ok.body))?
        .responses
        .into_iter()
        .filter(|entry| entry.status.is_none_or(|status| status / 100 != 2))
        .map(|entry| WebdavMemberFailure {
            href: entry.href,
            status: entry.status,
        })
        .collect();

    if failures.is_empty() {
        return Ok(ok);
    }

    Err(WebdavSendError::MemberFailures(failures))
}

/// Renders failed members as `href (status), href (status)`.
fn joined(failures: &[WebdavMemberFailure]) -> String {
    let failures: Vec<String> = failures.iter().map(ToString::to_string).collect();
    failures.join(", ")
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum State {
    Send(Http11Send),
    Reject(String),
}
