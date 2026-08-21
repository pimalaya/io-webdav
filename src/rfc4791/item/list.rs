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

use alloc::collections::BTreeSet;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::{
        calendar::calendar_query_body,
        item::{CaldavItemEntry, ITEM_PROPS, item_from_entry},
    },
    rfc4918::{WebdavAuth, report::WebdavReport, send::WebdavSendError},
    webdav_try,
};

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
    type Return = Result<BTreeSet<CaldavItemEntry>, WebdavSendError>;

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
