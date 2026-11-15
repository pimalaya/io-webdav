//! # Multiget items
//!
//! `multiget-items` coroutine: REPORT `calendar-multiget` against a calendar
//! collection (RFC 4791 §7.9).
//!
//! Fetches a batch of item bodies by resource name in a single round-trip,
//! instead of one GET per item. Stays byte-oriented: the iCalendar payload is
//! returned as raw bytes.
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
//!     rfc4791::item::multiget::CaldavItemMultiget,
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
//! let mut coroutine = CaldavItemMultiget::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     &["event-1.ics", "event-2.ics"],
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

use alloc::{string::String, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::{
        calendar::calendar_multiget_body,
        item::{CaldavItemEntry, CaldavItemProps, ITEM_PROPS, item_from_response, join_path},
    },
    rfc4918::{
        WebdavAuth,
        report::WebdavReport,
        send::WebdavSendError,
        typed::{WebdavMultistatusOf, WebdavUnreadableEntry},
    },
    webdav_try,
};

/// Successful terminal output of [`CaldavItemMultiget`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaldavItemMultigetOk {
    /// The fetched items, in the order met.
    pub items: Vec<CaldavItemEntry>,
    /// The items that came without their data (a 404 for a missing id), in
    /// the order met.
    pub unreadable: Vec<WebdavUnreadableEntry>,
}

/// Coroutine that batch-fetches calendar items by resource name via REPORT
/// `calendar-multiget`.
#[derive(Debug)]
pub struct CaldavItemMultiget {
    state: State,
}

impl CaldavItemMultiget {
    /// Builds a new `multiget-items` coroutine fetching each item of `ids`
    /// (resource ids, as a listing returns them) inside
    /// `calendar_path`. The `Depth` header is pinned to 0: RFC 4791 §7.9 only
    /// defines the report for that value.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        ids: &[&str],
    ) -> Self {
        let hrefs: Vec<String> = ids.iter().map(|id| join_path(calendar_path, id)).collect();
        let body = calendar_multiget_body(&hrefs, ITEM_PROPS);
        let report = WebdavReport::new(base_url, auth, user_agent, calendar_path, 0, body);
        Self {
            state: State::WebdavReport(report),
        }
    }
}

impl WebdavCoroutine for CaldavItemMultiget {
    type Yield = WebdavYield;
    type Return = Result<CaldavItemMultigetOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavReport(report) => {
                let multistatus = webdav_try!(report, arg);

                let mut items = Vec::new();
                let mut unreadable = Vec::new();

                for response in multistatus.responses {
                    match item_from_response(response) {
                        Some(Ok(item)) => items.push(item),
                        Some(Err(entry)) => unreadable.push(entry),
                        None => {}
                    }
                }

                WebdavCoroutineState::Complete(Ok(CaldavItemMultigetOk { items, unreadable }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavReport(WebdavReport<WebdavMultistatusOf<CaldavItemProps>>),
}
