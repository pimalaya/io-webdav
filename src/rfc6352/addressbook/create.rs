//! `create-addressbook` coroutine: extended `MKCOL` (RFC 5689)
//! against the addressbook home-set URL.
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
//!     rfc6352::addressbook::{CarddavAddressbook, create::CarddavAddressbookCreate},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let addressbook = CarddavAddressbook {
//!     id: "contacts".into(),
//!     display_name: Some("Contacts".into()),
//!     ..Default::default()
//! };
//! let mut coroutine =
//!     CarddavAddressbookCreate::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/", &addressbook);
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

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{WebdavAuth, mkcol::WebdavMkcol, send::WebdavSendError},
    rfc6352::addressbook::{ADDRESSBOOK, CarddavAddressbook, join_path, property_set},
};

/// Coroutine that creates an addressbook collection.
#[derive(Debug)]
pub struct CarddavAddressbookCreate {
    state: State,
}

impl CarddavAddressbookCreate {
    /// Builds a new `create-addressbook` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        addressbook: &CarddavAddressbook,
    ) -> Self {
        let path = join_path(home_set_path, &addressbook.id);
        let set = property_set(addressbook);
        let mkcol = WebdavMkcol::new(base_url, auth, user_agent, &path, &[ADDRESSBOOK], &set);
        Self {
            state: State::WebdavMkcol(mkcol),
        }
    }
}

impl WebdavCoroutine for CarddavAddressbookCreate {
    type Yield = WebdavYield;
    type Return = Result<(), WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavMkcol(mkcol) => mkcol.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavMkcol(WebdavMkcol),
}
