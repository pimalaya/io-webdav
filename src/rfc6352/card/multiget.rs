//! `multiget-cards` coroutine: REPORT `addressbook-multiget` against an
//! addressbook collection (RFC 6352 §8.7).
//!
//! Fetches a batch of card bodies by resource name in a single
//! round-trip, instead
//! of one GET per card. Stays byte-oriented: the vCard payload is
//! returned as raw bytes.
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
//!     rfc6352::card::multiget::MultigetCards,
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = MultigetCards::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/addressbooks/contacts/",
//!     &["alice", "bob"],
//! );
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

use alloc::{string::String, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{WebdavAuth, report::Report, send::SendError},
    rfc6352::{
        addressbook::addressbook_multiget_body,
        card::{
            types::CardEntry,
            utils::{CARD_PROPS, card_from_entry, join_path},
        },
    },
    webdav_try,
};

/// Coroutine that batch-fetches cards by resource name via REPORT
/// `addressbook-multiget`.
#[derive(Debug)]
pub struct MultigetCards {
    state: State,
}

impl MultigetCards {
    /// Builds a new `multiget-cards` coroutine fetching each card of
    /// `uris` (resource names as the server returned them) inside
    /// `addressbook_path`. The `Depth` header is pinned to 0: RFC 6352
    /// §8.7 only defines the report for that value.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        uris: &[&str],
    ) -> Self {
        let hrefs: Vec<String> = uris
            .iter()
            .map(|uri| join_path(addressbook_path, uri))
            .collect();
        let body = addressbook_multiget_body(&hrefs, CARD_PROPS);
        let report = Report::new(base_url, auth, user_agent, addressbook_path, 0, body);
        Self {
            state: State::Report(report),
        }
    }
}

impl WebdavCoroutine for MultigetCards {
    type Yield = WebdavYield;
    type Return = Result<Vec<CardEntry>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Report(report) => {
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
    Report(Report),
}
