//! `enum-items` coroutine: REPORT `calendar-query` requesting ETags
//! only, against a calendar collection.
//!
//! Enumerates the full item spine (id plus ETag) without downloading
//! any iCalendar body; bodies are then batch-fetched with
//! [`CaldavItemMultiget`](crate::rfc4791::item::multiget::CaldavItemMultiget).
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
//! // Ready stream needed (TCP-connected, TLS-negociated)
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
//! println!("{} items", refs.len());
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

/// Coroutine that enumerates item references (id plus ETag, no body)
/// inside a calendar via REPORT `calendar-query`.
#[derive(Debug)]
pub struct CaldavItemEnum {
    state: State,
}

impl CaldavItemEnum {
    /// Builds a new `enum-items` coroutine.
    ///
    /// `calendar_path` is the calendar collection path. `comp_filter`
    /// is the optional VCALENDAR child filter (e.g.
    /// `<C:comp-filter name="VEVENT" />`); pass an empty string to
    /// enumerate every component type.
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
    type Return = Result<BTreeSet<CaldavItemRef>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavReport(report) => {
                let multistatus = webdav_try!(report, arg);
                let refs = multistatus
                    .responses
                    .iter()
                    .filter_map(from_entry)
                    .collect();
                WebdavCoroutineState::Complete(Ok(refs))
            }
        }
    }
}

fn from_entry(entry: &WebdavResponseEntry) -> Option<CaldavItemRef> {
    // Skip the collection self-entry: a calendar object resource never
    // ends in a slash, but some servers (iCloud) echo the calendar
    // itself in the query response, which would otherwise enter the
    // spine as a bogus item named after the collection.
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
