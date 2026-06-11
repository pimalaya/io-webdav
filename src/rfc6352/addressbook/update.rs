//! `update-addressbook` coroutine: `PROPPATCH` against an
//! addressbook collection.
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
//!     rfc6352::addressbook::{Addressbook, update::UpdateAddressbook},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let addressbook = Addressbook {
//!     id: "contacts".into(),
//!     display_name: Some("My Contacts".into()),
//!     ..Default::default()
//! };
//! let mut coroutine =
//!     UpdateAddressbook::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/", &addressbook);
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
//!         WebdavCoroutineState::Complete(Ok(())) => break,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{WebdavAuth, proppatch::Proppatch, send::SendError},
    rfc6352::addressbook::{
        types::Addressbook,
        utils::{join_path, property_set},
    },
};

/// Coroutine that updates an addressbook collection's properties.
#[derive(Debug)]
pub struct UpdateAddressbook {
    state: State,
}

impl UpdateAddressbook {
    /// Builds a new `update-addressbook` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        addressbook: &Addressbook,
    ) -> Self {
        let path = join_path(home_set_path, &addressbook.id);
        let set = property_set(addressbook);
        let proppatch = Proppatch::new(base_url, auth, user_agent, &path, &set);
        Self {
            state: State::Proppatch(proppatch),
        }
    }
}

impl WebdavCoroutine for UpdateAddressbook {
    type Yield = WebdavYield;
    type Return = Result<(), SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Proppatch(proppatch) => proppatch.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    Proppatch(Proppatch),
}
