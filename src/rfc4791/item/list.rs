//! # List items
//!
//! `list-items` coroutine: REPORT `calendar-query` against a calendar
//! collection.
//!
//! Stays byte-oriented: the iCalendar payload is returned as raw bytes and
//! parsed upstream (ical).
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
//!     rfc4791::item::list::CaldavItemList,
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
//! let mut coroutine = CaldavItemList::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     "<C:comp-filter name=\"VEVENT\" />",
//! );
//! let mut arg = None;
//!
//! let ok = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(ok)) => break ok,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} items", ok.items.len());
//! ```

use alloc::{collections::BTreeSet, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::{
        calendar::calendar_query_body,
        item::{CaldavItemEntry, CaldavItemProps, ITEM_PROPS, item_from_response},
    },
    rfc4918::{
        WebdavAuth,
        report::WebdavReport,
        send::WebdavSendError,
        typed::{WebdavMultistatusOf, WebdavUnreadableEntry},
    },
    webdav_try,
};

/// Successful terminal output of [`CaldavItemList`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaldavItemListOk {
    /// The matching items.
    pub items: BTreeSet<CaldavItemEntry>,
    /// Whether the server truncated the result with a 507 row (RFC 6578
    /// §3.6), in which case [`items`](Self::items) is not every match.
    ///
    /// The row names the collection itself, which the self-entry skip would
    /// otherwise discard, so a partial listing would read as complete.
    pub truncated: bool,
    /// The items that came without their data, in the order met.
    pub unreadable: Vec<WebdavUnreadableEntry>,
}

/// Coroutine that lists items inside a calendar via REPORT `calendar-query`.
#[derive(Debug)]
pub struct CaldavItemList {
    state: State,
}

impl CaldavItemList {
    /// Builds a new `list-items` coroutine.
    ///
    /// `calendar_path` is the calendar collection path, `comp_filter` the
    /// optional VCALENDAR child filter, an empty string listing every component
    /// type.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        comp_filter: &str,
    ) -> Self {
        let body = calendar_query_body(ITEM_PROPS, comp_filter);
        let report = WebdavReport::new(base_url, auth, user_agent, calendar_path, 1, body);
        Self {
            state: State::WebdavReport(report),
        }
    }
}

impl WebdavCoroutine for CaldavItemList {
    type Yield = WebdavYield;
    type Return = Result<CaldavItemListOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavReport(report) => {
                let multistatus = webdav_try!(report, arg);

                let truncated = multistatus
                    .responses
                    .iter()
                    .any(|entry| entry.status == Some(507));

                let mut items = BTreeSet::new();
                let mut unreadable = Vec::new();

                for response in multistatus.responses {
                    match item_from_response(response) {
                        Some(Ok(item)) => {
                            items.insert(item);
                        }
                        Some(Err(entry)) => unreadable.push(entry),
                        None => {}
                    }
                }

                WebdavCoroutineState::Complete(Ok(CaldavItemListOk {
                    items,
                    truncated,
                    unreadable,
                }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavReport(WebdavReport<WebdavMultistatusOf<CaldavItemProps>>),
}
