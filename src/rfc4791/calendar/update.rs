//! `update-calendar` coroutine: `PROPPATCH` against a calendar
//! collection.
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
//!     rfc4791::calendar::{Calendar, update::UpdateCalendar},
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
//! let calendar = Calendar {
//!     id: "personal".into(),
//!     color: Some("#ff0000".into()),
//!     ..Default::default()
//! };
//! let mut coroutine =
//!     UpdateCalendar::new(&base_url, &auth, "io-webdav", "/dav/calendars/", &calendar);
//! let mut arg = None;
//!
//! loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(())) => break,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use core::fmt;

use alloc::string::String;

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::{
        types::Calendar,
        utils::{format_body, join_path},
    },
    rfc4918::{
        proppatch::Proppatch,
        send::{SendError, SendOk},
        {MkcolResponse, WebdavAuth},
    },
    webdav_try,
};

const BODY: &str = include_str!("./update.xml");

/// Coroutine that updates a calendar collection's properties.
#[derive(Debug)]
pub struct UpdateCalendar {
    state: State,
}

impl UpdateCalendar {
    /// Builds a new `update-calendar` coroutine targeting
    /// `home_set_path` joined with `calendar.id`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        calendar: &Calendar,
    ) -> Self {
        let path = join_path(home_set_path, &calendar.id);
        let body = format_body(BODY, calendar).into_bytes();
        Self {
            state: State::Proppatch(Proppatch::new(base_url, auth, user_agent, &path, body)),
        }
    }
}

impl WebdavCoroutine for UpdateCalendar {
    type Yield = WebdavYield;
    type Return = Result<(), SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("update-calendar: {}", self.state);
        match &mut self.state {
            State::Proppatch(proppatch) => {
                let ok = webdav_try!(proppatch, arg);
                log_propstats(&ok);
                WebdavCoroutineState::Complete(Ok(()))
            }
        }
    }
}

fn log_propstats(ok: &SendOk<MkcolResponse<Prop>>) {
    let Some(propstats) = &ok.body.propstats else {
        return;
    };

    for propstat in propstats {
        if !propstat.status.is_success() {
            trace!("skip propstat with non-2xx status");
            continue;
        }

        if let Some(name) = &propstat.prop.displayname {
            trace!("calendar displayname updated: {name}");
        }

        if let Some(desc) = &propstat.prop.calendar_description {
            trace!("calendar description updated: {desc}");
        }

        if let Some(color) = &propstat.prop.calendar_color {
            trace!("calendar color updated: {color}");
        }
    }
}

#[derive(Debug)]
enum State {
    Proppatch(Proppatch<Prop>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Proppatch(_) => f.write_str("proppatch"),
        }
    }
}

/// `<prop>` payload echoed by a `PROPPATCH` response.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub displayname: Option<String>,
    pub calendar_color: Option<String>,
    pub calendar_description: Option<String>,
}
