//! `create-calendar` coroutine: extended `MKCOL` (RFC 5689) against
//! the calendar home-set URL.
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
//!     rfc4791::calendar::{Calendar, create::CreateCalendar},
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
//!     display_name: Some("Personal".into()),
//!     ..Default::default()
//! };
//! let mut coroutine =
//!     CreateCalendar::new(&base_url, &auth, "io-webdav", "/dav/calendars/", &calendar);
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
//!         WebdavCoroutineState::Complete(Ok(_)) => break,
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
        utils::{format_body, join_path},
    },
    rfc4918::{
        WebdavAuth,
        mkcol::Mkcol,
        send::{Empty, SendError, SendOk},
    },
};

const BODY: &str = include_str!("./create.xml");

/// Coroutine that creates a calendar collection.
#[derive(Debug)]
pub struct CreateCalendar {
    state: State,
}

impl CreateCalendar {
    /// Builds a new `create-calendar` coroutine targeting
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
            state: State::Mkcol(Mkcol::new(base_url, auth, user_agent, &path, body)),
        }
    }
}

impl WebdavCoroutine for CreateCalendar {
    type Yield = WebdavYield;
    type Return = Result<SendOk<Empty>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("create-calendar: {}", self.state);
        match &mut self.state {
            State::Mkcol(mkcol) => mkcol.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    Mkcol(Mkcol),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mkcol(_) => f.write_str("mkcol"),
        }
    }
}
