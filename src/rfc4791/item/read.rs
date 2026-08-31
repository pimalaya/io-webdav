//! # Read item
//!
//! `read-item` coroutine: GET a calendar item by its resource name.
//!
//! Stays byte-oriented: returns raw iCalendar bytes plus the response's `ETag`,
//! leaving the parse to ical upstream.
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
//!     rfc4791::item::read::CaldavItemRead,
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
//! let mut coroutine =
//!     CaldavItemRead::new(&base_url, &auth, "io-webdav", "/dav/calendars/personal/", "event-1.ics");
//! let mut arg = None;
//!
//! let item = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(item)) => break item,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} bytes, etag {:?}", item.data.len(), item.etag);
//! ```

use alloc::{string::String, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::item::join_path,
    rfc4918::{
        WebdavAuth,
        get::WebdavGet,
        read_etag,
        send::{WebdavSendError, WebdavSendOk},
    },
    webdav_try,
};

/// Coroutine that reads a calendar item.
#[derive(Debug)]
pub struct CaldavItemRead {
    state: State,
}

impl CaldavItemRead {
    /// Builds a new `read-item` coroutine. `id` is the resource id exactly as
    /// the server returned it (`CaldavItemEntry::id`), used verbatim.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        id: &str,
    ) -> Self {
        let path = join_path(calendar_path, id);
        Self {
            state: State::WebdavGet(WebdavGet::new(base_url, auth, user_agent, &path)),
        }
    }
}

impl WebdavCoroutine for CaldavItemRead {
    type Yield = WebdavYield;
    type Return = Result<CaldavItemBody, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavGet(get) => {
                let WebdavSendOk { response, body, .. } = webdav_try!(get, arg);
                let etag = read_etag(&response);
                WebdavCoroutineState::Complete(Ok(CaldavItemBody { data: body, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavGet(WebdavGet),
}

/// Item body plus optional ETag returned by [`CaldavItemRead`].
#[derive(Clone, Debug)]
pub struct CaldavItemBody {
    /// Raw iCalendar bytes.
    pub data: Vec<u8>,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}
