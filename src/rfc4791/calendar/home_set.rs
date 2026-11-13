//! # Calendar home-set
//!
//! `calendar-home-set` discovery (RFC 4791 §6.2.1).
//!
//! Runs a PROPFIND against the principal URL and surfaces the discovered
//! calendar-home-set URL.
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
//!     rfc4791::calendar::home_set::CaldavCalendarHomeSet,
//!     rfc4918::{WebdavAuth, coroutine::WebdavRedirectYield},
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine =
//!     CaldavCalendarHomeSet::new(&base_url, &auth, "io-webdav", "/principals/alice/");
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

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::CALENDAR_HOME_SET,
    rfc4918::{
        WebdavAuth, WebdavHrefs, body_text,
        coroutine::WebdavRedirectYield,
        follow_redirects::{WebdavFollowRedirects, WebdavFollowRedirectsError},
        propfind_body,
        request::WebdavRequest,
        resolve_href,
        typed::parse_multistatus_as,
    },
    webdav_try,
};

/// I/O-free coroutine that discovers the calendar-home-set URL. Yields the
/// first of several, and [`None`] when the server names none.
#[derive(Debug)]
pub struct CaldavCalendarHomeSet {
    base_url: Url,
    state: State,
}

impl CaldavCalendarHomeSet {
    /// Builds a new `calendar-home-set` discovery coroutine targeting
    /// `principal_path` (relative to `base_url`).
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, principal_path: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, principal_path)
            .depth(0)
            .content_type_xml()
            .body(propfind_body(&[CALENDAR_HOME_SET]));

        Self {
            base_url: base_url.clone(),
            state: State::Send(WebdavFollowRedirects::new(request)),
        }
    }
}

impl WebdavCoroutine for CaldavCalendarHomeSet {
    type Yield = WebdavRedirectYield;
    type Return = Result<Option<Url>, WebdavFollowRedirectsError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                let xml = body_text(&ok.body);
                let multistatus = match parse_multistatus_as::<HomeSetProps>(&xml) {
                    Ok(multistatus) => multistatus,
                    Err(err) => return WebdavCoroutineState::Complete(Err(err.into())),
                };
                let url = multistatus
                    .responses
                    .into_iter()
                    .filter_map(|response| response.props.calendar_home_set)
                    .flat_map(|home_set| home_set.href)
                    .find(|href| !href.is_empty())
                    .and_then(|href| resolve_href(&self.base_url, &href));
                WebdavCoroutineState::Complete(Ok(url))
            }
        }
    }
}

/// The `calendar-home-set` (RFC 4791 §6.2.1), as a typed multistatus reads it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct HomeSetProps {
    calendar_home_set: Option<WebdavHrefs>,
}

#[derive(Debug)]
enum State {
    Send(WebdavFollowRedirects),
}
