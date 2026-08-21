//! `delete-item` coroutine: `DELETE` a calendar item by its resource name.
//!
//! Supports the optional `If-Match` precondition so callers can gate the
//! deletion on the last-known ETag (RFC 9110 §13.1.1).
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
//!     rfc4791::item::delete::CaldavItemDelete,
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
//! let mut coroutine = CaldavItemDelete::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     "event-1.ics",
//!     None,
//! );
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
    rfc4791::item::join_path,
    rfc4918::{
        WebdavAuth,
        delete::WebdavDelete,
        send::{WebdavSendError, WebdavSendOk},
    },
};

/// Coroutine that deletes a calendar item.
#[derive(Debug)]
pub struct CaldavItemDelete {
    state: State,
}

impl CaldavItemDelete {
    /// Builds a new `delete-item` coroutine. `id` is the resource id exactly as
    /// the server returned it (`CaldavItemRef::id`), used verbatim.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        id: &str,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(calendar_path, id);
        Self {
            state: State::WebdavDelete(WebdavDelete::new(
                base_url, auth, user_agent, &path, if_match,
            )),
        }
    }
}

impl WebdavCoroutine for CaldavItemDelete {
    type Yield = WebdavYield;
    type Return = Result<WebdavSendOk<Vec<u8>>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavDelete(delete) => delete.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavDelete(WebdavDelete),
}
