//! # Update addressbook
//!
//! `update-addressbook` coroutine: `PROPPATCH` against an addressbook
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
//!     rfc6352::addressbook::{CarddavAddressbookPatch, update::CarddavAddressbookUpdate},
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//!
//! // Renames the addressbook and clears its description, leaving its
//! // color untouched.
//! let patch = CarddavAddressbookPatch {
//!     id: "contacts".into(),
//!     display_name: Some(Some("My Contacts".into())),
//!     description: Some(None),
//!     ..Default::default()
//! };
//! let mut coroutine =
//!     CarddavAddressbookUpdate::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/", &patch);
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
//!         WebdavCoroutineState::Complete(Ok(out)) => {
//!             // Each refused property is listed under `failures`, and
//!             // `requested` is what the request asked to change.
//!             println!("{:?}", out.multistatus.responses);
//!             break;
//!         }
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        proppatch::{WebdavProppatch, WebdavProppatchOk},
        send::WebdavSendError,
    },
    rfc6352::addressbook::{CarddavAddressbookPatch, join_path, property_updates},
};

/// Coroutine that updates an addressbook collection's properties.
#[derive(Debug)]
pub struct CarddavAddressbookUpdate {
    state: State,
}

impl CarddavAddressbookUpdate {
    /// Builds a new `update-addressbook` coroutine, setting the properties
    /// `patch` carries a value for and removing the ones it clears.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        patch: &CarddavAddressbookPatch,
    ) -> Self {
        let path = join_path(home_set_path, &patch.id);
        let (set, remove) = property_updates(patch);
        let proppatch = WebdavProppatch::new(base_url, auth, user_agent, &path, &set, &remove);
        Self {
            state: State::WebdavProppatch(proppatch),
        }
    }
}

impl WebdavCoroutine for CarddavAddressbookUpdate {
    type Yield = WebdavYield;
    type Return = Result<WebdavProppatchOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavProppatch(proppatch) => proppatch.resume(arg),
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavProppatch(WebdavProppatch),
}
