//! # List cards
//!
//! `list-cards` coroutine: REPORT `addressbook-query` against an addressbook
//! collection, filtered and capped by [`CarddavCardListOptions`].
//!
//! Stays byte-oriented: the vCard payload is returned as raw bytes and parsed
//! upstream (vcard).
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
//!     rfc6352::card::list::{CarddavCardList, CarddavCardListOptions},
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let path = "/dav/addressbooks/contacts/";
//! let opts = CarddavCardListOptions::default();
//! let mut coroutine = CarddavCardList::new(&base_url, &auth, "io-webdav", path, &opts);
//! let mut arg = None;
//!
//! let ok = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(ok)) => break ok,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} cards", ok.cards.len());
//! ```

use core::num::NonZeroU32;

use alloc::collections::BTreeSet;

use log::{debug, trace};
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{WebdavAuth, has_element, report::WebdavReport, send::WebdavSendError},
    rfc6352::{
        addressbook::addressbook_query_body,
        card::{CARD_PROPS, CarddavCardEntry, card_from_entry},
        filter::CarddavFilter,
    },
};

/// Options for [`CarddavCardList::new`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CarddavCardListOptions {
    /// The query filter, matching every card by default.
    pub filter: CarddavFilter,
    /// The most cards the server should return (RFC 6352 §8.6.1).
    pub limit: Option<NonZeroU32>,
}

/// Successful terminal output of [`CarddavCardList`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CarddavCardListOk {
    /// The matching cards.
    pub cards: BTreeSet<CarddavCardEntry>,
    /// Whether the server truncated the result with a 507 row (RFC 6352
    /// §8.6.1), in which case [`cards`](Self::cards) is not every match.
    ///
    /// The row names the collection itself, which the self-entry skip would
    /// otherwise discard, so a partial listing would read as complete.
    pub truncated: bool,
}

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
        opts: &CarddavCardListOptions,
    ) -> Self {
        let body = addressbook_query_body(CARD_PROPS, &opts.filter, opts.limit);
        let report = WebdavReport::new(base_url, auth, user_agent, addressbook_path, 1, body);
        Self {
            state: State::WebdavReport(report),
        }
    }
}

impl WebdavCoroutine for CarddavCardList {
    type Yield = WebdavYield;
    type Return = Result<CarddavCardListOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavReport(report) => {
                let multistatus = match report.resume(arg) {
                    WebdavCoroutineState::Yielded(yielded) => {
                        return WebdavCoroutineState::Yielded(yielded);
                    }
                    WebdavCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(unsupported_filter(err)));
                    }
                    WebdavCoroutineState::Complete(Ok(multistatus)) => multistatus,
                };

                let truncated = multistatus
                    .responses
                    .iter()
                    .any(|entry| entry.status == Some(507));
                let cards = multistatus
                    .responses
                    .iter()
                    .filter_map(card_from_entry)
                    .collect();

                WebdavCoroutineState::Complete(Ok(CarddavCardListOk { cards, truncated }))
            }
        }
    }
}

/// Turns a send failure saying the server cannot evaluate the filter into
/// [`WebdavSendError::UnsupportedFilter`], and leaves every other one alone.
///
/// RFC 6352 §8.6 names both refusals as preconditions and recommends no
/// status, so the element is what is matched.
fn unsupported_filter(err: WebdavSendError) -> WebdavSendError {
    let WebdavSendError::HttpStatus { status, body } = err else {
        return err;
    };

    if has_element(&body, &[SUPPORTED_FILTER, SUPPORTED_COLLATION]) {
        debug!("WebDAV server does not support the query filter");
        return WebdavSendError::UnsupportedFilter { status, body };
    }

    WebdavSendError::HttpStatus { status, body }
}

/// Local name of the precondition refusing a property, parameter or match
/// type (RFC 6352 §8.6).
const SUPPORTED_FILTER: &str = "supported-filter";

/// Local name of the precondition refusing a collation (RFC 6352 §8.6).
const SUPPORTED_COLLATION: &str = "supported-collation";

#[derive(Debug)]
enum State {
    WebdavReport(WebdavReport),
}
