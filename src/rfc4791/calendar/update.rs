//! # Update calendar
//!
//! `update-calendar` coroutine: `PROPPATCH` against a calendar collection.
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
//!     rfc4791::calendar::{CaldavCalendarPatch, update::CaldavCalendarUpdate},
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
//!
//! // Sets the calendar's color and clears its description, leaving its
//! // display name untouched.
//! let patch = CaldavCalendarPatch {
//!     id: "personal".into(),
//!     color: Some(Some("#ff0000".into())),
//!     description: Some(None),
//!     ..Default::default()
//! };
//! let mut coroutine =
//!     CaldavCalendarUpdate::new(&base_url, &auth, "io-webdav", "/dav/calendars/", &patch);
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
    rfc4791::calendar::{CaldavCalendarPatch, join_path, property_updates},
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
    /// Builds a new `update-calendar` coroutine, setting the properties
    /// `patch` carries a value for and removing the ones it clears.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        patch: &CaldavCalendarPatch,
    ) -> Self {
        let path = join_path(home_set_path, &patch.id);
        let (set, remove) = property_updates(patch);
        let proppatch = WebdavProppatch::new(base_url, auth, user_agent, &path, &set, &remove);
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
