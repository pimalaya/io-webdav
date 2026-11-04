//! # List calendars
//!
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

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::{CALENDAR, CaldavCalendar, LIST_PROPS},
    rfc4918::{
        WebdavAuth, WebdavChildNames, WebdavReportSet, collection_id, non_empty,
        propfind::WebdavPropfind,
        send::WebdavSendError,
        typed::{WebdavMultistatusOf, WebdavResponseOf},
    },
    webdav_try,
};

/// Coroutine that lists calendars under `home_set_path`.
#[derive(Debug)]
pub struct CaldavCalendarList {
    home_set_path: String,
    state: State,
}

impl CaldavCalendarList {
    /// Builds a new `list-calendars` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, home_set_path: &str) -> Self {
        let propfind =
            WebdavPropfind::new(base_url, auth, user_agent, home_set_path, 1, LIST_PROPS);
        Self {
            home_set_path: home_set_path.to_string(),
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
                    .into_iter()
                    .filter_map(|response| from_response(response, &self.home_set_path))
                    .collect();
                WebdavCoroutineState::Complete(Ok(calendars))
            }
        }
    }
}

/// The properties of [`LIST_PROPS`], as a typed multistatus reads them.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CalendarProps {
    resourcetype: Option<WebdavChildNames>,
    displayname: Option<String>,
    calendar_description: Option<String>,
    calendar_color: Option<String>,
    supported_calendar_component_set: Option<ComponentSet>,
    getctag: Option<String>,
    sync_token: Option<String>,
    supported_report_set: Option<WebdavReportSet>,
    calendar_timezone: Option<String>,
}

/// A `supported-calendar-component-set` (RFC 4791 §5.2.3), each type the
/// `name` attribute of a `comp` child.
#[derive(Debug, Default, Deserialize)]
struct ComponentSet {
    #[serde(default)]
    comp: Vec<Component>,
}

#[derive(Debug, Default, Deserialize)]
struct Component {
    #[serde(rename = "@name")]
    name: Option<String>,
}

fn from_response(
    response: WebdavResponseOf<CalendarProps>,
    home_set_path: &str,
) -> Option<CaldavCalendar> {
    let props = response.props;

    if !props
        .resourcetype
        .is_some_and(|types| types.0.contains_key(CALENDAR.local))
    {
        trace!("skip non-calendar response {}", response.href);
        return None;
    }

    let id = collection_id(home_set_path, &response.href);
    if id.is_empty() {
        return None;
    }

    let components = props
        .supported_calendar_component_set
        .map(|set| set.comp.into_iter().filter_map(|comp| comp.name).collect())
        .unwrap_or_default();

    Some(CaldavCalendar {
        id,
        display_name: non_empty(props.displayname),
        description: non_empty(props.calendar_description),
        color: non_empty(props.calendar_color),
        components,
        ctag: non_empty(props.getctag),
        sync_token: non_empty(props.sync_token),
        tz: non_empty(props.calendar_timezone),
        supported_reports: props
            .supported_report_set
            .map(WebdavReportSet::reports)
            .unwrap_or_default(),
    })
}

#[derive(Debug)]
enum State {
    WebdavPropfind(WebdavPropfind<WebdavMultistatusOf<CalendarProps>>),
}
