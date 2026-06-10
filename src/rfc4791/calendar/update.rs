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

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::{
        types::Calendar,
        utils::{join_path, property_set},
    },
    rfc4918::{WebdavAuth, proppatch::Proppatch, send::SendError},
};

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
        let set = property_set(calendar);
        let proppatch = Proppatch::new(base_url, auth, user_agent, &path, &set);
        Self {
            state: State::Proppatch(proppatch),
        }
    }
}

impl WebdavCoroutine for UpdateCalendar {
    type Yield = WebdavYield;
    type Return = Result<(), SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("update-calendar: {}", self.state);
        match &mut self.state {
            State::Proppatch(proppatch) => proppatch.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    Proppatch(Proppatch),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Proppatch(_) => f.write_str("proppatch"),
        }
    }
}
