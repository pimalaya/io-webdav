//! `enum-items` coroutine: REPORT `calendar-query` requesting ETags only,
//! against a calendar collection.
//!
//! Enumerates the full item spine (id plus ETag) without downloading any
//! iCalendar body; bodies are then batch-fetched with
//! [`CaldavItemMultiget`](crate::rfc4791::item::multiget::CaldavItemMultiget).
//! A 507 row flags the listing truncated, so a partial spine is never taken for
//! the whole calendar.
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
//!     rfc4791::item::enumerate::CaldavItemEnum,
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
//! let mut coroutine = CaldavItemEnum::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     "<C:comp-filter name=\"VEVENT\" />",
//! );
//! let mut arg = None;
//!
//! let refs = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(refs)) => break refs,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} items", refs.refs.len());
//! ```

use alloc::{collections::BTreeSet, string::ToString};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::{calendar::calendar_query_body, item::CaldavItemRef},
    rfc4918::{
        GETETAG, WebdavAuth, WebdavProperty, WebdavResponseEntry, report::WebdavReport,
        send::WebdavSendError,
    },
    webdav_try,
};

const ENUM_PROPS: &[WebdavProperty] = &[GETETAG];

/// Successful terminal output of [`CaldavItemEnum`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaldavItemEnumOk {
    /// The enumerated item references (id plus ETag, no body).
    pub refs: BTreeSet<CaldavItemRef>,
    /// Whether the server truncated the listing with a 507 row (RFC 6578 §3.6),
    /// in which case [`refs`](Self::refs) is a part of the calendar and not the
    /// whole of it.
    ///
    /// A full enumeration is how removals are detected without a sync token, so
    /// a consumer taking a truncated one for a complete snapshot reads the
    /// missing members as deletions.
    pub truncated: bool,
}

/// Coroutine that enumerates item references (id plus ETag, no body) inside a
/// calendar via REPORT `calendar-query`.
#[derive(Debug)]
pub struct CaldavItemEnum {
    state: State,
}

impl CaldavItemEnum {
    /// Builds a new `enum-items` coroutine.
    ///
    /// `calendar_path` is the calendar collection path, `comp_filter` the
    /// optional VCALENDAR child filter, an empty string enumerating every
    /// component type.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        comp_filter: &str,
    ) -> Self {
        let body = calendar_query_body(ENUM_PROPS, comp_filter);
        let report = WebdavReport::new(base_url, auth, user_agent, calendar_path, 1, body);
        Self {
            state: State::WebdavReport(report),
        }
    }
}

impl WebdavCoroutine for CaldavItemEnum {
    type Yield = WebdavYield;
    type Return = Result<CaldavItemEnumOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavReport(report) => {
                let multistatus = webdav_try!(report, arg);

                let truncated = multistatus
                    .responses
                    .iter()
                    .any(|entry| entry.status == Some(507));
                let refs = multistatus
                    .responses
                    .iter()
                    .filter_map(from_entry)
                    .collect();

                WebdavCoroutineState::Complete(Ok(CaldavItemEnumOk { refs, truncated }))
            }
        }
    }
}

fn from_entry(entry: &WebdavResponseEntry) -> Option<CaldavItemRef> {
    // NOTE: an item href never ends in a slash, but some servers (iCloud) echo
    // the calendar itself, which would otherwise enter the spine as a bogus
    // item named after the collection.
    if entry.href.ends_with('/') {
        return None;
    }

    let id = entry.id();
    if id.is_empty() {
        return None;
    }

    Some(CaldavItemRef {
        id: id.to_string(),
        etag: entry
            .text(GETETAG)
            .map(|raw| raw.trim_matches('"').to_string()),
    })
}

#[derive(Debug)]
enum State {
    WebdavReport(WebdavReport),
}
