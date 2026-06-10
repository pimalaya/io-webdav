//! `update-item` coroutine: PUT raw iCalendar bytes against an
//! existing calendar item.
//!
//! Supports the optional `If-Match` precondition so callers can gate
//! the write on the last-known ETag (RFC 9110 §13.1.1).
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
//!     rfc4791::item::update::UpdateItem,
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
//! let ical = b"BEGIN:VCALENDAR\r\n...\r\nEND:VCALENDAR\r\n".to_vec();
//! let mut coroutine = UpdateItem::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     "event-1",
//!     ical,
//!     Some("\"abc123\""),
//! );
//! let mut arg = None;
//!
//! let updated = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(updated)) => break updated,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("updated {} (etag {:?})", updated.id, updated.etag);
//! ```

use core::{fmt, mem};

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::item::{types::UpdateItemOk, utils::join_path},
    rfc4918::{
        WebdavAuth,
        put::{Put, PutArgs},
        read_etag,
        send::{SendError, SendOk},
    },
    webdav_try,
};

/// Coroutine that updates a calendar item.
#[derive(Debug)]
pub struct UpdateItem {
    id: String,
    state: State,
}

impl UpdateItem {
    /// Builds a new `update-item` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        id: &str,
        ical: Vec<u8>,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(calendar_path, id);
        let put = Put::new(PutArgs {
            base_url,
            auth,
            user_agent,
            path: &path,
            content_type: "text/calendar; charset=utf-8",
            body: ical,
            if_match,
            if_none_match: None,
        });
        Self {
            id: id.to_string(),
            state: State::Put(put),
        }
    }
}

impl WebdavCoroutine for UpdateItem {
    type Yield = WebdavYield;
    type Return = Result<UpdateItemOk, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("update-item: {}", self.state);
        match &mut self.state {
            State::Put(put) => {
                let SendOk { response, .. } = webdav_try!(put, arg);
                let etag = read_etag(&response);
                let id = mem::take(&mut self.id);
                WebdavCoroutineState::Complete(Ok(UpdateItemOk { id, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Put(Put),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Put(_) => f.write_str("put"),
        }
    }
}
