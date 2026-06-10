//! `current-user-principal` discovery (RFC 5397).
//!
//! Runs a `PROPFIND` against `/` with the `<DAV:current-user-principal>`
//! property request and surfaces the discovered principal URL. Yields
//! [`WantsRedirect`] since the entry path is usually `/` and servers
//! often redirect to the actual DAV root.
//!
//! [`WantsRedirect`]: crate::rfc4918::coroutine::WebdavRedirectYield::WantsRedirect
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
//!     rfc4918::{WebdavAuth, coroutine::WebdavRedirectYield},
//!     rfc5397::current_user_principal::CurrentUserPrincipal,
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = CurrentUserPrincipal::new(&base_url, &auth, "io-webdav");
//! let mut arg = None;
//!
//! let principal = loop {
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
//!         WebdavCoroutineState::Complete(Ok(principal)) => break principal,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{principal:?}");
//! ```

use core::fmt;

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        coroutine::WebdavRedirectYield,
        follow_redirects::{FollowRedirects, FollowRedirectsError},
        request::WebdavRequest,
        send::SendOk,
        {HrefProp, Multistatus, WebdavAuth},
    },
    webdav_try,
};

const BODY: &str = include_str!("./current_user_principal.xml");

/// I/O-free coroutine that discovers the current user principal URL.
/// Yields [`None`] when the server returned an empty multistatus.
#[derive(Debug)]
pub struct CurrentUserPrincipal {
    base_url: Url,
    state: State,
}

impl CurrentUserPrincipal {
    /// Builds a new `current-user-principal` coroutine targeting `/`
    /// against `base_url`.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, "/")
            .content_type_xml()
            .body(BODY.as_bytes().to_vec());

        Self {
            base_url: base_url.clone(),
            state: State::Send(FollowRedirects::new(request)),
        }
    }
}

impl WebdavCoroutine for CurrentUserPrincipal {
    type Yield = WebdavRedirectYield;
    type Return = Result<Option<Url>, FollowRedirectsError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("current-user-principal: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                let url = first_principal(&ok, &self.base_url);
                WebdavCoroutineState::Complete(Ok(url))
            }
        }
    }
}

fn first_principal(ok: &SendOk<Multistatus<Prop>>, base_url: &Url) -> Option<Url> {
    let responses = ok.body.responses.as_ref()?;

    for response in responses {
        if let Some(status) = &response.status {
            if !status.is_success() {
                trace!("skip multistatus response with non-2xx status");
                continue;
            }
        }

        let Some(propstats) = &response.propstats else {
            continue;
        };

        for propstat in propstats {
            if !propstat.status.is_success() {
                trace!("skip propstat with non-2xx status");
                continue;
            }

            if let Ok(url) = propstat.prop.current_user_principal.url(base_url) {
                return Some(url);
            }
        }
    }

    None
}

#[derive(Debug)]
enum State {
    Send(FollowRedirects<Multistatus<Prop>>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

/// `<prop>` payload returned by the principal discovery PROPFIND.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub current_user_principal: HrefProp,
}
