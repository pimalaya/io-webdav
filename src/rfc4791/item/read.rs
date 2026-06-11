//! `read-item` coroutine: GET a calendar item by id.
//!
//! Stays byte-oriented: returns raw iCalendar bytes plus the
//! response's `ETag` so io-calendar can run calcard upstream.
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
//!     rfc4791::item::read::ReadItem,
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
//!     ReadItem::new(&base_url, &auth, "io-webdav", "/dav/calendars/personal/", "event-1");
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

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::item::{types::ItemBody, utils::join_path},
    rfc4918::{
        WebdavAuth,
        get::Get,
        read_etag,
        send::{SendError, SendOk},
    },
    webdav_try,
};

/// Coroutine that reads a calendar item.
#[derive(Debug)]
pub struct ReadItem {
    state: State,
}

impl ReadItem {
    /// Builds a new `read-item` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        item_id: &str,
    ) -> Self {
        let path = join_path(calendar_path, item_id);
        Self {
            state: State::Get(Get::new(base_url, auth, user_agent, &path)),
        }
    }
}

impl WebdavCoroutine for ReadItem {
    type Yield = WebdavYield;
    type Return = Result<ItemBody, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Get(get) => {
                let SendOk { response, body, .. } = webdav_try!(get, arg);
                let etag = read_etag(&response);
                WebdavCoroutineState::Complete(Ok(ItemBody { data: body, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Get(Get),
}
