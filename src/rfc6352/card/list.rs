//! `list-cards` coroutine: REPORT `addressbook-query` against an
//! addressbook collection.
//!
//! Stays byte-oriented: the vCard payload is returned as raw bytes
//! and parsed by io-addressbook.
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
//!     rfc6352::card::list::ListCards,
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
//!     ListCards::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/contacts/");
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

use core::fmt;

use alloc::{collections::BTreeSet, string::ToString};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        GETETAG, WebdavAuth,
        report::Report,
        send::SendError,
        trace_unrecognized, {Property, ResponseEntry},
    },
    rfc6352::{
        addressbook::{ADDRESS_DATA, addressbook_query_body},
        card::types::CardEntry,
    },
    webdav_try,
};

const CARD_PROPS: &[Property] = &[GETETAG, ADDRESS_DATA];

/// Coroutine that lists cards inside an addressbook via REPORT
/// `addressbook-query`.
#[derive(Debug)]
pub struct ListCards {
    state: State,
}

impl ListCards {
    /// Builds a new `list-cards` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
    ) -> Self {
        let body = addressbook_query_body(CARD_PROPS);
        let report = Report::new(base_url, auth, user_agent, addressbook_path, 1, body);
        Self {
            state: State::Report(report),
        }
    }
}

impl WebdavCoroutine for ListCards {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<CardEntry>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("list-cards: {}", self.state);
        match &mut self.state {
            State::Report(report) => {
                let multistatus = webdav_try!(report, arg);
                let cards = multistatus
                    .responses
                    .iter()
                    .filter_map(from_entry)
                    .collect();
                WebdavCoroutineState::Complete(Ok(cards))
            }
        }
    }
}

fn from_entry(entry: &ResponseEntry) -> Option<CardEntry> {
    let id = entry.id().trim_end_matches(".vcf");
    if id.is_empty() {
        return None;
    }

    let data = entry.text(ADDRESS_DATA)?;
    trace_unrecognized(entry, CARD_PROPS);

    let etag = entry
        .text(GETETAG)
        .map(|raw| raw.trim_matches('"').to_string());

    Some(CardEntry {
        id: id.to_string(),
        etag,
        data: data.as_bytes().to_vec(),
    })
}

#[derive(Debug)]
enum State {
    Report(Report),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Report(_) => f.write_str("report"),
        }
    }
}
