//! # List addressbooks
//!
//! `list-addressbooks` coroutine: PROPFIND Depth:1 against the addressbook
//! home-set URL, collecting every child collection whose resourcetype is
//! `<C:addressbook/>`.
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
//!     rfc6352::addressbook::list::CarddavAddressbookList,
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
//!     CarddavAddressbookList::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/");
//! let mut arg = None;
//!
//! let addressbooks = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(addressbooks)) => break addressbooks,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} addressbooks", addressbooks.len());
//! ```

use alloc::{collections::BTreeSet, string::ToString};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        DISPLAYNAME, GETCTAG, RESOURCETYPE, SYNC_TOKEN, WebdavAuth, WebdavResponseEntry,
        propfind::WebdavPropfind, send::WebdavSendError, trace_unrecognized,
    },
    rfc6352::addressbook::{
        ADDRESSBOOK, ADDRESSBOOK_COLOR, ADDRESSBOOK_DESCRIPTION, CarddavAddressbook, LIST_PROPS,
    },
    webdav_try,
};

/// Coroutine that lists addressbooks under `home_set_path`.
#[derive(Debug)]
pub struct CarddavAddressbookList {
    state: State,
}

impl CarddavAddressbookList {
    /// Builds a new `list-addressbooks` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, home_set_path: &str) -> Self {
        let propfind =
            WebdavPropfind::new(base_url, auth, user_agent, home_set_path, 1, LIST_PROPS);
        Self {
            state: State::WebdavPropfind(propfind),
        }
    }
}

impl WebdavCoroutine for CarddavAddressbookList {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<CarddavAddressbook>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavPropfind(propfind) => {
                let multistatus = webdav_try!(propfind, arg);
                let addressbooks = multistatus
                    .responses
                    .iter()
                    .filter_map(from_entry)
                    .collect();
                WebdavCoroutineState::Complete(Ok(addressbooks))
            }
        }
    }
}

fn from_entry(entry: &WebdavResponseEntry) -> Option<CarddavAddressbook> {
    if !entry.has_resource_type(RESOURCETYPE, ADDRESSBOOK) {
        trace!("skip non-addressbook response {}", entry.href);
        return None;
    }

    let id = entry.id();
    if id.is_empty() {
        return None;
    }

    trace_unrecognized(entry, LIST_PROPS);

    Some(CarddavAddressbook {
        id: id.to_string(),
        display_name: entry.text(DISPLAYNAME).map(ToString::to_string),
        description: entry.text(ADDRESSBOOK_DESCRIPTION).map(ToString::to_string),
        color: entry.text(ADDRESSBOOK_COLOR).map(ToString::to_string),
        ctag: entry.text(GETCTAG).map(ToString::to_string),
        sync_token: entry.text(SYNC_TOKEN).map(ToString::to_string),
        supported_reports: entry.supported_reports(),
    })
}

#[derive(Debug)]
enum State {
    WebdavPropfind(WebdavPropfind),
}
