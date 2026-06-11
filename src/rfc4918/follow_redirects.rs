//! Send coroutine that surfaces 3xx redirects to the caller.
//!
//! Runs an HTTP/1.1 exchange and turns the underlying
//! `HttpSendYield::WantsRedirect` into a
//! [`WebdavRedirectYield::WantsRedirect`] so the client can rebuild its
//! connection and restart the operation against the new target URL. The
//! success body is returned raw; callers parse it with
//! [`parse_multistatus`](crate::rfc4918::parse_multistatus).
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
//! let mut coroutine = FollowRedirects::new(request);
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
//! println!("{} bytes", ok.body.len());
//! ```

use alloc::{string::String, vec::Vec};

use io_http::{
    coroutine::*,
    rfc9110::{request::HttpRequest, send::HttpSendOutput},
    rfc9112::send::{Http11Send, Http11SendError},
};
use log::trace;
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

    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// I/O-free coroutine that sends a WebDAV request, surfaces 3xx
/// redirects via [`WebdavRedirectYield::WantsRedirect`] and returns the
/// success body as raw bytes.
#[derive(Debug)]
pub struct FollowRedirects {
    state: State,
}

impl FollowRedirects {
    /// Builds a new redirect-aware send coroutine. `request` must
    /// already carry its body bytes.
    pub fn new(request: HttpRequest) -> Self {
        Self {
            state: State::Send(Http11Send::new(request)),
        }
    }
}

impl WebdavCoroutine for FollowRedirects {
    type Yield = WebdavRedirectYield;
    type Return = Result<SendOk<Vec<u8>>, FollowRedirectsError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
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

                if !response.status.is_success() {
                    let body = String::from_utf8_lossy(&response.body).into_owned();
                    let err = FollowRedirectsError::HttpStatus(*response.status, body);
                    return WebdavCoroutineState::Complete(Err(err));
                }

                let body = response.body.clone();
                WebdavCoroutineState::Complete(Ok(SendOk {
                    response,
                    keep_alive,
                    body,
                }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Send(Http11Send),
}
