//! `addressbook-home-set` discovery (RFC 6352 §7.1.1).
//!
//! Runs a PROPFIND against the principal URL and surfaces the
//! discovered addressbook-home-set URL.
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
//!     rfc6352::addressbook::home_set::AddressbookHomeSet,
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
//!     AddressbookHomeSet::new(&base_url, &auth, "io-webdav", "/principals/alice/");
//! let mut arg = None;
//!
//! let home_set = loop {
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
//!         WebdavCoroutineState::Complete(Ok(home_set)) => break home_set,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{home_set:?}");
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

const BODY: &str = include_str!("./home_set.xml");

/// I/O-free coroutine that discovers the addressbook-home-set URL.
/// Yields [`None`] when the server returned an empty multistatus.
#[derive(Debug)]
pub struct AddressbookHomeSet {
    base_url: Url,
    state: State,
}

impl AddressbookHomeSet {
    /// Builds a new `addressbook-home-set` discovery coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, principal_path: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, principal_path)
            .content_type_xml()
            .body(BODY.as_bytes().to_vec());

        Self {
            base_url: base_url.clone(),
            state: State::Send(FollowRedirects::new(request)),
        }
    }
}

impl WebdavCoroutine for AddressbookHomeSet {
    type Yield = WebdavRedirectYield;
    type Return = Result<Option<Url>, FollowRedirectsError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("addressbook-home-set: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                let url = first_home_set(&ok, &self.base_url);
                WebdavCoroutineState::Complete(Ok(url))
            }
        }
    }
}

fn first_home_set(ok: &SendOk<Multistatus<Prop>>, base_url: &Url) -> Option<Url> {
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

            if let Ok(url) = propstat.prop.addressbook_home_set.url(base_url) {
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

/// `<prop>` payload returned by the addressbook-home-set discovery
/// PROPFIND.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub addressbook_home_set: HrefProp,
}
