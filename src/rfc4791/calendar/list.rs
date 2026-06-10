//! `list-calendars` coroutine: PROPFIND Depth:1 against the calendar
//! home-set URL, collecting every child collection whose resourcetype
//! is `<C:calendar/>`.
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
//!     rfc4791::calendar::list::ListCalendars,
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
//! let mut coroutine = ListCalendars::new(&base_url, &auth, "io-webdav", "/dav/calendars/");
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
    rfc4791::calendar::types::Calendar,
    rfc4918::{
        request::WebdavRequest,
        send::{Send, SendError, SendOk},
        {Multistatus, WebdavAuth},
    },
    webdav_try,
};

const BODY: &str = include_str!("./list.xml");

/// Coroutine that lists calendars under `home_set_path`.
#[derive(Debug)]
pub struct ListCalendars {
    state: State,
}

impl ListCalendars {
    /// Builds a new `list-calendars` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, home_set_path: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, home_set_path)
            .depth(1)
            .content_type_xml()
            .body(BODY.as_bytes().to_vec());
        Self {
            state: State::Send(Send::new(request)),
        }
    }
}

impl WebdavCoroutine for ListCalendars {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<Calendar>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("list-calendars: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                WebdavCoroutineState::Complete(Ok(collect(&ok)))
            }
        }
    }
}

fn collect(ok: &SendOk<Multistatus<Prop>>) -> BTreeSet<Calendar> {
    let mut calendars = BTreeSet::new();

    let Some(responses) = &ok.body.responses else {
        return calendars;
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
            .to_string();

        let mut calendar = Calendar {
            id,
            ..Default::default()
        };
        let mut is_calendar = false;

        for propstat in propstats {
            if !propstat.status.is_success() {
                trace!("skip propstat with non-2xx status");
                continue;
            }

            if let Some(rtype) = &propstat.prop.resourcetype {
                if rtype.calendar.is_some() {
                    is_calendar = true;
                }
            }

            if let Some(name) = non_empty(propstat.prop.displayname.as_deref()) {
                calendar.display_name = Some(name);
            }

            if let Some(desc) = non_empty(propstat.prop.calendar_description.as_deref()) {
                calendar.description = Some(desc);
            }

            if let Some(color) = non_empty(propstat.prop.calendar_color.as_deref()) {
                calendar.color = Some(color);
            }

            if let Some(ctag) = non_empty(propstat.prop.getctag.as_deref()) {
                calendar.ctag = Some(ctag);
            }

            if let Some(tz) = non_empty(propstat.prop.calendar_timezone.as_deref()) {
                calendar.tz = Some(tz);
            }
        }

        if is_calendar && !calendar.id.is_empty() {
            calendars.insert(calendar);
        }
    }

    calendars
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
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

/// `<prop>` payload returned by the list-calendars PROPFIND.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub resourcetype: Option<ResourceType>,
    pub displayname: Option<String>,
    pub calendar_color: Option<String>,
    pub calendar_description: Option<String>,
    pub getctag: Option<String>,
    pub calendar_timezone: Option<String>,
}

/// `<resourcetype>` element returned by the list-calendars PROPFIND.
#[derive(Clone, Debug, Deserialize)]
pub struct ResourceType {
    pub calendar: Option<()>,
}
