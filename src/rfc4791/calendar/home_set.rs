//! `calendar-home-set` discovery (RFC 4791 §6.2.1).
//!
//! Runs a PROPFIND against the principal URL and surfaces the
//! discovered calendar-home-set URL.
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
//!     rfc4791::calendar::home_set::CalendarHomeSet,
//!     rfc4918::{WebdavAuth, coroutine::WebdavRedirectYield},
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
//!     CalendarHomeSet::new(&base_url, &auth, "io-webdav", "/principals/alice/");
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

use alloc::string::String;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::CALENDAR_HOME_SET,
    rfc4918::{
        WebdavAuth,
        coroutine::WebdavRedirectYield,
        follow_redirects::{FollowRedirects, FollowRedirectsError},
        parse_multistatus, propfind_body,
        request::WebdavRequest,
        resolve_href,
    },
    webdav_try,
};

/// I/O-free coroutine that discovers the calendar-home-set URL. Yields
/// [`None`] when the server returned an empty multistatus.
#[derive(Debug)]
pub struct CalendarHomeSet {
    base_url: Url,
    state: State,
}

impl CalendarHomeSet {
    /// Builds a new `calendar-home-set` discovery coroutine targeting
    /// `principal_path` (relative to `base_url`).
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, principal_path: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, principal_path)
            .depth(0)
            .content_type_xml()
            .body(propfind_body(&[CALENDAR_HOME_SET]));

        Self {
            base_url: base_url.clone(),
            state: State::Send(FollowRedirects::new(request)),
        }
    }
}

impl WebdavCoroutine for CalendarHomeSet {
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
                    .find_map(|entry| entry.text(CALENDAR_HOME_SET))
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
