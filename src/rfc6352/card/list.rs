//! `list-cards` coroutine: REPORT `addressbook-query` against an
//! addressbook collection.
//!
//! Stays byte-oriented: the vCard payload is returned as raw bytes
//! and parsed upstream (vcard).
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
//!     rfc6352::card::list::CarddavCardList,
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
//!     CarddavCardList::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/contacts/");
//! let mut arg = None;
//!
//! let cards = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(cards)) => break cards,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} cards", cards.len());
//! ```

use alloc::collections::BTreeSet;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{WebdavAuth, report::WebdavReport, send::WebdavSendError},
    rfc6352::{
        addressbook::addressbook_query_body,
        card::{CARD_PROPS, CarddavCardEntry, card_from_entry},
    },
    webdav_try,
};

/// Coroutine that lists cards inside an addressbook via REPORT
/// `addressbook-query`.
#[derive(Debug)]
pub struct CarddavCardList {
    state: State,
}

impl CarddavCardList {
    /// Builds a new `list-cards` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
    ) -> Self {
        let body = addressbook_query_body(CARD_PROPS);
        let report = WebdavReport::new(base_url, auth, user_agent, addressbook_path, 1, body);
        Self {
            state: State::WebdavReport(report),
        }
    }
}

impl WebdavCoroutine for CarddavCardList {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<CarddavCardEntry>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavReport(report) => {
                let multistatus = webdav_try!(report, arg);
                let cards = multistatus
                    .responses
                    .iter()
                    .filter_map(card_from_entry)
                    .collect();
                WebdavCoroutineState::Complete(Ok(cards))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavReport(WebdavReport),
}
