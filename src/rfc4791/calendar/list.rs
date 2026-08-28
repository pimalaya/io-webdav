//! `list-calendars` coroutine: PROPFIND Depth:1 against the calendar home-set
//! URL, collecting every child collection whose resourcetype is
//! `<C:calendar/>`.
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
//!     rfc4791::calendar::list::CaldavCalendarList,
//!     rfc4918::WebdavAuth,
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = CaldavCalendarList::new(&base_url, &auth, "io-webdav", "/dav/calendars/");
//! let mut arg = None;
//!
//! let calendars = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(calendars)) => break calendars,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} calendars", calendars.len());
//! ```

use alloc::{collections::BTreeSet, string::ToString};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::{
        CALENDAR, CALENDAR_COLOR, CALENDAR_DESCRIPTION, CALENDAR_TIMEZONE, CaldavCalendar,
        LIST_PROPS, SUPPORTED_CALENDAR_COMPONENT_SET,
    },
    rfc4918::{
        DISPLAYNAME, GETCTAG, RESOURCETYPE, SYNC_TOKEN, WebdavAuth, WebdavResponseEntry,
        propfind::WebdavPropfind, send::WebdavSendError, trace_unrecognized,
    },
    webdav_try,
};

/// Coroutine that lists calendars under `home_set_path`.
#[derive(Debug)]
pub struct CaldavCalendarList {
    state: State,
}

impl CaldavCalendarList {
    /// Builds a new `list-calendars` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, home_set_path: &str) -> Self {
        let propfind =
            WebdavPropfind::new(base_url, auth, user_agent, home_set_path, 1, LIST_PROPS);
        Self {
            state: State::WebdavPropfind(propfind),
        }
    }
}

impl WebdavCoroutine for CaldavCalendarList {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<CaldavCalendar>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavPropfind(propfind) => {
                let multistatus = webdav_try!(propfind, arg);
                let calendars = multistatus
                    .responses
                    .iter()
                    .filter_map(from_entry)
                    .collect();
                WebdavCoroutineState::Complete(Ok(calendars))
            }
        }
    }
}

fn from_entry(entry: &WebdavResponseEntry) -> Option<CaldavCalendar> {
    if !entry.has_resource_type(RESOURCETYPE, CALENDAR) {
        trace!("skip non-calendar response {}", entry.href);
        return None;
    }

    let id = entry.id();
    if id.is_empty() {
        return None;
    }

    trace_unrecognized(entry, LIST_PROPS);

    // NOTE: the component set is markup, not text: each type is the `name`
    // attribute of a `<C:comp/>` child (RFC 4791 §5.2.3).
    let components = entry
        .prop(SUPPORTED_CALENDAR_COMPONENT_SET)
        .map(|item| {
            item.children
                .iter()
                .filter_map(|child| child.name.clone())
                .collect()
        })
        .unwrap_or_default();

    Some(CaldavCalendar {
        id: id.to_string(),
        display_name: entry.text(DISPLAYNAME).map(ToString::to_string),
        description: entry.text(CALENDAR_DESCRIPTION).map(ToString::to_string),
        color: entry.text(CALENDAR_COLOR).map(ToString::to_string),
        components,
        ctag: entry.text(GETCTAG).map(ToString::to_string),
        sync_token: entry.text(SYNC_TOKEN).map(ToString::to_string),
        tz: entry.text(CALENDAR_TIMEZONE).map(ToString::to_string),
        supported_reports: entry.supported_reports(),
    })
}

#[derive(Debug)]
enum State {
    WebdavPropfind(WebdavPropfind),
}
