//! `delete-item` coroutine: `DELETE` a calendar item by id.
//!
//! Supports the optional `If-Match` precondition so callers can gate
//! the deletion on the last-known ETag (RFC 9110 §13.1.1).
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
//!     rfc4791::item::delete::DeleteItem,
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
//! let mut coroutine = DeleteItem::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     "event-1",
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
    rfc4791::item::utils::join_path,
    rfc4918::{
        WebdavAuth,
        delete::Delete,
        send::{SendError, SendOk},
    },
};

/// Coroutine that deletes a calendar item.
#[derive(Debug)]
pub struct DeleteItem {
    state: State,
}

impl DeleteItem {
    /// Builds a new `delete-item` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        item_id: &str,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(calendar_path, item_id);
        Self {
            state: State::Delete(Delete::new(base_url, auth, user_agent, &path, if_match)),
        }
    }
}

impl WebdavCoroutine for DeleteItem {
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
