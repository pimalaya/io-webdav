//! `delete-addressbook` coroutine: `DELETE` against an addressbook
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
//!     rfc4918::WebdavAuth,
//!     rfc6352::addressbook::delete::DeleteAddressbook,
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
//!     DeleteAddressbook::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/", "contacts");
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

use core::fmt;

use alloc::vec::Vec;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        delete::Delete,
        send::{SendError, SendOk},
    },
    rfc6352::addressbook::utils::join_path,
};

/// Coroutine that deletes an addressbook collection.
#[derive(Debug)]
pub struct DeleteAddressbook {
    state: State,
}

impl DeleteAddressbook {
    /// Builds a new `delete-addressbook` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        addressbook_id: &str,
    ) -> Self {
        let path = join_path(home_set_path, addressbook_id);
        Self {
            state: State::Delete(Delete::new(base_url, auth, user_agent, &path, None)),
        }
    }
}

impl WebdavCoroutine for DeleteAddressbook {
    type Yield = WebdavYield;
    type Return = Result<SendOk<Vec<u8>>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("delete-addressbook: {}", self.state);
        match &mut self.state {
            State::Delete(delete) => delete.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    Delete(Delete),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Delete(_) => f.write_str("delete"),
        }
    }
}
