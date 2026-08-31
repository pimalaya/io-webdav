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
//! let items = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(items)) => break items,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} items", items.len());
//! ```

use alloc::{string::String, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::{
        calendar::calendar_multiget_body,
        item::{CaldavItemEntry, ITEM_PROPS, item_from_entry, join_path},
    },
    rfc4918::{WebdavAuth, report::WebdavReport, send::WebdavSendError},
    webdav_try,
};

/// Coroutine that batch-fetches calendar items by resource name via REPORT
/// `calendar-multiget`.
#[derive(Debug)]
pub struct CaldavItemMultiget {
    state: State,
}

impl CaldavItemMultiget {
    /// Builds a new `multiget-items` coroutine fetching each item of `ids`
    /// (resource ids as the server returned them, used verbatim) inside
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
    type Return = Result<Vec<CaldavItemEntry>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavReport(report) => {
                let multistatus = webdav_try!(report, arg);
                let items = multistatus
                    .responses
                    .iter()
                    .filter_map(item_from_entry)
                    .collect();
                WebdavCoroutineState::Complete(Ok(items))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavReport(WebdavReport),
}
