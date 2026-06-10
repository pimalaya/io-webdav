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

use core::fmt;

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::item::types::ItemEntry,
    rfc4918::{
        request::WebdavRequest,
        send::{Send, SendError, SendOk},
        {Multistatus, Value, WebdavAuth},
    },
    webdav_try,
};

const BODY: &str = include_str!("./list.xml");

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
        let body = BODY.replacen("{}", comp_filter, 1).into_bytes();

        let request = WebdavRequest::report(base_url, auth, user_agent, calendar_path)
            .content_type_xml()
            .depth(1)
            .body(body);

        Self {
            state: State::Send(Send::new(request)),
        }
    }
}

impl WebdavCoroutine for ListItems {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<ItemEntry>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("list-items: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                WebdavCoroutineState::Complete(Ok(collect(&ok)))
            }
        }
    }
}

fn collect(ok: &SendOk<Multistatus<Prop>>) -> BTreeSet<ItemEntry> {
    let mut items = BTreeSet::new();

    let Some(responses) = &ok.body.responses else {
        return items;
    };

    for response in responses {
        trace!("process multistatus response");

        if let Some(status) = &response.status {
            if !status.is_success() {
                trace!("skip multistatus response with non-2xx status");
                continue;
            }
        }

        let Some(propstats) = &response.propstats else {
            continue;
        };

        let id = response
            .href
            .value
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".ics")
            .to_string();

        if id.is_empty() {
            continue;
        }

        for propstat in propstats {
            if !propstat.status.is_success() {
                trace!("skip propstat with non-2xx status");
                continue;
            }

            let Some(data) = &propstat.prop.calendar_data else {
                continue;
            };

            let etag = propstat
                .prop
                .getetag
                .as_deref()
                .map(|raw| raw.trim_matches('"').to_string());

            items.insert(ItemEntry {
                id: id.clone(),
                etag,
                data: data.value.as_bytes().to_vec(),
            });

            break;
        }
    }

    items
}

#[derive(Debug)]
enum State {
    Send(Send<Multistatus<Prop>>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

/// `<prop>` payload returned by the list-items REPORT.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub getetag: Option<String>,
    pub calendar_data: Option<Value>,
}
