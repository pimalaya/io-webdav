//! # Update item
//!
//! `update-item` coroutine: PUT raw iCalendar bytes against an existing
//! calendar item.
//!
//! Supports the optional `If-Match` precondition so callers can gate the write
//! on the last-known ETag (RFC 9110 §13.1.1).
//!
//! A collection that already holds the item's `UID` under another resource
//! refuses the PUT with the `CALDAV:no-uid-conflict` precondition (RFC 4791
//! §5.3.2), surfaced as
//! [`DuplicateUid`](crate::rfc4918::send::WebdavSendError::DuplicateUid).
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
//!     rfc4791::item::update::CaldavItemUpdate,
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
//! let ical = b"BEGIN:VCALENDAR\r\n...\r\nEND:VCALENDAR\r\n".to_vec();
//! let mut coroutine = CaldavItemUpdate::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     "event-1.ics",
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

use core::mem;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::item::join_path,
    rfc4918::{
        WebdavAuth,
        put::{WebdavPut, WebdavPutArgs, duplicate_uid},
        read_etag,
        send::{WebdavSendError, WebdavSendOk},
    },
};

/// Coroutine that updates a calendar item.
#[derive(Debug)]
pub struct CaldavItemUpdate {
    id: String,
    state: State,
}

impl CaldavItemUpdate {
    /// Builds a new `update-item` coroutine. `id` is the resource id exactly as
    /// the server returned it (`CaldavItemEntry::id`), used verbatim.
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
        let put = WebdavPut::new(WebdavPutArgs {
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
            state: State::WebdavPut(put),
        }
    }
}

impl WebdavCoroutine for CaldavItemUpdate {
    type Yield = WebdavYield;
    type Return = Result<CaldavItemUpdateOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavPut(put) => {
                let ok = match put.resume(arg) {
                    WebdavCoroutineState::Yielded(yielded) => {
                        return WebdavCoroutineState::Yielded(yielded);
                    }
                    WebdavCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(duplicate_uid(err)));
                    }
                    WebdavCoroutineState::Complete(Ok(ok)) => ok,
                };

                let WebdavSendOk { response, .. } = ok;
                let etag = read_etag(&response);
                let id = mem::take(&mut self.id);
                WebdavCoroutineState::Complete(Ok(CaldavItemUpdateOk { id, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavPut(WebdavPut),
}

/// Outcome of a successful [`CaldavItemUpdate`] resume.
#[derive(Clone, Debug)]
pub struct CaldavItemUpdateOk {
    /// Item resource id (the resource name supplied by the caller, used
    /// verbatim).
    pub id: String,
    /// Updated entity tag returned by the server, when present.
    pub etag: Option<String>,
}
