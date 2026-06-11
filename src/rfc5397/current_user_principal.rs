//! `current-user-principal` discovery (RFC 5397).
//!
//! Runs a `PROPFIND` against the base URL with the
//! `<DAV:current-user-principal>` property request and surfaces the
//! discovered principal URL. The base URL must point at a DAV resource
//! (the server root for servers that serve DAV at `/`, or the DAV
//! context path such as `/dav/` otherwise). Yields [`WantsRedirect`]
//! when the server redirects to the actual DAV root.
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

use alloc::string::String;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        DAV, Property, WebdavAuth,
        coroutine::WebdavRedirectYield,
        follow_redirects::{FollowRedirects, FollowRedirectsError},
        parse_multistatus, propfind_body,
        request::WebdavRequest,
        resolve_href,
    },
    webdav_try,
};

/// `DAV:current-user-principal` property (RFC 5397 §3).
pub const CURRENT_USER_PRINCIPAL: Property = Property {
    ns: DAV,
    local: "current-user-principal",
};

/// I/O-free coroutine that discovers the current user principal URL.
/// Yields [`None`] when the server returned an empty multistatus.
#[derive(Debug)]
pub struct CurrentUserPrincipal {
    base_url: Url,
    state: State,
}

impl CurrentUserPrincipal {
    /// Builds a new `current-user-principal` coroutine targeting
    /// `base_url`'s own path.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, "")
            .depth(0)
            .content_type_xml()
            .body(propfind_body(&[CURRENT_USER_PRINCIPAL]));

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
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                let xml = String::from_utf8_lossy(&ok.body);
                let url = parse_multistatus(&xml)
                    .responses
                    .iter()
                    .find_map(|entry| entry.text(CURRENT_USER_PRINCIPAL))
                    .and_then(|href| resolve_href(&self.base_url, href));
                WebdavCoroutineState::Complete(Ok(url))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Send(FollowRedirects),
}
