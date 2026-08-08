//! `create-item` coroutine: PUT raw iCalendar bytes against
//! `<calendar>/<id>`.
//!
//! The `id` is the resource name, used verbatim. io-webdav never
//! appends a file extension, so the caller owns the whole name. The
//! returned [`CaldavItemCreateOk::id`] is the caller's name, or the server's
//! own when it relocates the resource and reports it in a `Location`
//! header. Either way it is what read, update and delete address.
//!
//! Uses `If-None-Match: *` so the server rejects the PUT when a
//! resource with the same id already exists (RFC 4791 §5.3.2).
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
//!     rfc4791::item::create::CaldavItemCreate,
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
//! let mut coroutine = CaldavItemCreate::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     "event-1.ics",
//!     ical,
//! );
//! let mut arg = None;
//!
//! let created = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(created)) => break created,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("created {} (etag {:?})", created.id, created.etag);
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
        WebdavAuth, id_from_location,
        put::{WebdavPut, WebdavPutArgs},
        read_etag,
        send::{WebdavSendError, WebdavSendOk},
    },
    webdav_try,
};

/// Coroutine that creates a calendar item.
#[derive(Debug)]
pub struct CaldavItemCreate {
    id: String,
    state: State,
}

impl CaldavItemCreate {
    /// Builds a new `create-item` coroutine targeting
    /// `<calendar_path>/<id>`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        id: &str,
        ical: Vec<u8>,
    ) -> Self {
        let path = join_path(calendar_path, id);
        let put = WebdavPut::new(WebdavPutArgs {
            base_url,
            auth,
            user_agent,
            path: &path,
            content_type: "text/calendar; charset=utf-8",
            body: ical,
            if_match: None,
            if_none_match: Some("*"),
        });
        Self {
            id: id.to_string(),
            state: State::WebdavPut(put),
        }
    }
}

impl WebdavCoroutine for CaldavItemCreate {
    type Yield = WebdavYield;
    type Return = Result<CaldavItemCreateOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavPut(put) => {
                let WebdavSendOk { response, .. } = webdav_try!(put, arg);
                let etag = read_etag(&response);
                let id = response
                    .header("location")
                    .and_then(id_from_location)
                    .unwrap_or_else(|| mem::take(&mut self.id));
                WebdavCoroutineState::Complete(Ok(CaldavItemCreateOk { id, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavPut(WebdavPut),
}

/// Outcome of a successful
/// [`CaldavItemCreate`] resume.
#[derive(Clone, Debug)]
pub struct CaldavItemCreateOk {
    /// Item resource id: the `Location` header's last path segment when
    /// the server returns one (its own name for the resource), otherwise
    /// the caller-supplied name, verbatim.
    pub id: String,
    /// Entity tag returned by the server, when present.
    pub etag: Option<String>,
}
