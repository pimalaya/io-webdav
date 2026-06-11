//! `delete-calendar` coroutine: `DELETE` against a calendar
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
//!     rfc4791::calendar::delete::DeleteCalendar,
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
//! let mut coroutine =
//!     DeleteCalendar::new(&base_url, &auth, "io-webdav", "/dav/calendars/", "personal");
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

use alloc::vec::Vec;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::calendar::utils::join_path,
    rfc4918::{
        WebdavAuth,
        delete::Delete,
        send::{SendError, SendOk},
    },
};

/// Coroutine that deletes a calendar collection.
#[derive(Debug)]
pub struct DeleteCalendar {
    state: State,
}

impl DeleteCalendar {
    /// Builds a new `delete-calendar` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        calendar_id: &str,
    ) -> Self {
        let path = join_path(home_set_path, calendar_id);
        Self {
            state: State::Delete(Delete::new(base_url, auth, user_agent, &path, None)),
        }
    }
}

impl WebdavCoroutine for DeleteCalendar {
    type Yield = WebdavYield;
    type Return = Result<SendOk<Vec<u8>>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Delete(delete) => delete.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    Delete(Delete),
}
