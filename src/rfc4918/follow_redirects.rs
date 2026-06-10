//! Send coroutine that surfaces 3xx redirects to the caller.
//!
//! Runs an HTTP/1.1 exchange and turns the underlying
//! `HttpSendYield::WantsRedirect` into a
//! [`WebdavRedirectYield::WantsRedirect`] so the client can rebuild its
//! connection and restart the operation against the new target URL.
//!
//! [`WebdavRedirectYield::WantsRedirect`]: crate::rfc4918::coroutine::WebdavRedirectYield::WantsRedirect
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
//!     coroutine::{WebdavCoroutine, WebdavCoroutineState},
//!     rfc4918::{
//!         WebdavAuth,
//!         coroutine::WebdavRedirectYield,
//!         follow_redirects::FollowRedirects,
//!         request::WebdavRequest,
//!         send::Empty,
//!     },
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let request = WebdavRequest::propfind(&base_url, &auth, "io-webdav", "/")
//!     .content_type_xml()
//!     .body(Vec::new());
//! let mut coroutine = FollowRedirects::<Empty>::new(request);
//! let mut arg = None;
//!
//! let ok = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRedirect { url, .. }) => {
//!             todo!("reconnect to {url}");
//!         }
//!         WebdavCoroutineState::Complete(Ok(ok)) => break ok,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("keep-alive: {}", ok.keep_alive);
//! ```

use core::{fmt, marker::PhantomData};

use alloc::string::String;

use io_http::{
    coroutine::*,
    rfc9110::{request::HttpRequest, response::HttpResponse, send::HttpSendOutput},
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::trace;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    coroutine::*,
    rfc4918::{coroutine::WebdavRedirectYield, send::SendOk},
};

/// Failure causes during a redirect-aware WebDAV send.
#[derive(Debug, Error)]
pub enum FollowRedirectsError {
    #[error("WebDAV server returned HTTP {0}: {1}")]
    HttpStatus(u16, String),
    #[error("Parse WebDAV XML response body error: {0}")]
    ParseXmlResponseBody(#[source] quick_xml::DeError),

    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// I/O-free coroutine that sends a WebDAV request, surfaces 3xx
/// redirects via [`WebdavRedirectYield::WantsRedirect`] and parses the
/// success body as XML into `T`.
#[derive(Debug)]
pub struct FollowRedirects<T: for<'a> Deserialize<'a>> {
    phantom: PhantomData<T>,
    state: State,
}

impl<T: for<'a> Deserialize<'a>> FollowRedirects<T> {
    /// Builds a new redirect-aware send coroutine. `request` must
    /// already carry its body bytes.
    pub fn new(request: HttpRequest) -> Self {
        trace!("send WebDAV request to {} (redirect-aware)", request.url);

        Self {
            phantom: PhantomData,
            state: State::Send(Http11Send::new(request)),
        }
    }
}

impl<T: for<'a> Deserialize<'a>> WebdavCoroutine for FollowRedirects<T> {
    type Yield = WebdavRedirectYield;
    type Return = Result<SendOk<T>, FollowRedirectsError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("follow redirects: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let out = match send.resume(arg) {
                    HttpCoroutineState::Yielded(y) => {
                        return WebdavCoroutineState::Yielded(y.into());
                    }
                    HttpCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(err.into()));
                    }
                    HttpCoroutineState::Complete(Ok(out)) => out,
                };

                let HttpSendOutput {
                    response,
                    keep_alive,
                    ..
                } = out;

                parse_ok(response, keep_alive)
            }
        }
    }
}

fn parse_ok<T: for<'a> Deserialize<'a>>(
    response: HttpResponse,
    keep_alive: bool,
) -> WebdavCoroutineState<WebdavRedirectYield, Result<SendOk<T>, FollowRedirectsError>> {
    let body = String::from_utf8_lossy(&response.body);
    trace!("WebDAV response body: {body}");

    if !response.status.is_success() {
        let err = FollowRedirectsError::HttpStatus(*response.status, body.into_owned());
        return WebdavCoroutineState::Complete(Err(err));
    }

    let parsed = match quick_xml::de::from_str::<T>(&body) {
        Ok(parsed) => parsed,
        Err(err) => {
            let err = FollowRedirectsError::ParseXmlResponseBody(err);
            return WebdavCoroutineState::Complete(Err(err));
        }
    };

    WebdavCoroutineState::Complete(Ok(SendOk {
        response,
        keep_alive,
        body: parsed,
    }))
}

#[derive(Debug)]
enum State {
    Send(Http11Send),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}
