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
//!     rfc4791::calendar::{CaldavCalendar, update::CaldavCalendarUpdate},
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
//! let calendar = CaldavCalendar {
//!     id: "personal".into(),
//!     color: Some("#ff0000".into()),
//!     ..Default::default()
//! };
//! let mut coroutine =
//!     CaldavCalendarUpdate::new(&base_url, &auth, "io-webdav", "/dav/calendars/", &calendar);
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
//!         WebdavCoroutineState::Complete(Ok(out)) => {
//!             // Each refused property is listed under `failures`, and
//!             // `requested` is what the request asked to change.
//!             println!("{:?}", out.multistatus.responses);
//!             break;
//!         }
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::{CaldavCalendar, join_path, property_set},
    rfc4918::{
        WebdavAuth,
        proppatch::{WebdavProppatch, WebdavProppatchOk},
        send::WebdavSendError,
    },
};

/// Coroutine that updates a calendar collection's properties.
#[derive(Debug)]
pub struct CaldavCalendarUpdate {
    state: State,
}

impl CaldavCalendarUpdate {
    /// Builds a new `update-calendar` coroutine targeting
    /// `home_set_path` joined with `calendar.id`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        calendar: &CaldavCalendar,
    ) -> Self {
        let path = join_path(home_set_path, &calendar.id);
        let set = property_set(calendar);
        // NOTE: set-only, so a property this calendar leaves empty stays
        // as it is on the server. The CardDAV twin takes a patch that
        // can also remove; this one follows when calendula needs it.
        let proppatch = WebdavProppatch::new(base_url, auth, user_agent, &path, &set, &[]);
        Self {
            state: State::WebdavProppatch(proppatch),
        }
    }
}

impl WebdavCoroutine for CaldavCalendarUpdate {
    type Yield = WebdavYield;
    type Return = Result<WebdavProppatchOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavProppatch(proppatch) => proppatch.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavProppatch(WebdavProppatch),
}
