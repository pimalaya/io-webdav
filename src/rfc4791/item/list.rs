//! `list-items` coroutine: REPORT `calendar-query` against a calendar
//! collection.
//!
//! Stays byte-oriented: the iCalendar payload is returned as raw bytes
//! and parsed by io-calendar.
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
//!     rfc4791::item::list::ListItems,
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
//! let mut coroutine = ListItems::new(
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

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::{CALENDAR_DATA, calendar_query_body},
    rfc4918::{
        GETETAG, WebdavAuth,
        report::Report,
        send::SendError,
        trace_unrecognized, {Property, ResponseEntry},
    },
    webdav_try,
};

const ITEM_PROPS: &[Property] = &[GETETAG, CALENDAR_DATA];

/// Coroutine that lists items inside a calendar via REPORT
/// `calendar-query`.
#[derive(Debug)]
pub struct ListItems {
    state: State,
}

impl ListItems {
    /// Builds a new `list-items` coroutine.
    ///
    /// `calendar_path` is the calendar collection path. `comp_filter`
    /// is the optional VCALENDAR child filter (e.g.
    /// `<C:comp-filter name="VEVENT" />`); pass an empty string to
    /// list every component type.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        comp_filter: &str,
    ) -> Self {
        let body = calendar_query_body(ITEM_PROPS, comp_filter);
        let report = Report::new(base_url, auth, user_agent, calendar_path, 1, body);
        Self {
            state: State::Report(report),
        }
    }
}

impl WebdavCoroutine for ListItems {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<ItemEntry>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Report(report) => {
                let multistatus = webdav_try!(report, arg);
                let items = multistatus
                    .responses
                    .iter()
                    .filter_map(from_entry)
                    .collect();
                WebdavCoroutineState::Complete(Ok(items))
            }
        }
    }
}

fn from_entry(entry: &ResponseEntry) -> Option<ItemEntry> {
    let id = entry.id().trim_end_matches(".ics");
    if id.is_empty() {
        return None;
    }

    let data = entry.text(CALENDAR_DATA)?;
    trace_unrecognized(entry, ITEM_PROPS);

    let etag = entry
        .text(GETETAG)
        .map(|raw| raw.trim_matches('"').to_string());

    Some(ItemEntry {
        id: id.to_string(),
        etag,
        data: data.as_bytes().to_vec(),
    })
}

#[derive(Debug)]
enum State {
    Report(Report),
}

/// Raw calendar item entry returned by
/// [`ListItems`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemEntry {
    /// Item identifier (last path segment of the href, with `.ics`
    /// stripped).
    pub id: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,

    /// Raw iCalendar bytes (`calendar-data`).
    pub data: Vec<u8>,
}
