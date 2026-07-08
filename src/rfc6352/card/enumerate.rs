//! `enum-cards` coroutine: REPORT `addressbook-query` requesting ETags
//! only, against an addressbook collection.
//!
//! Enumerates the full card spine (id plus ETag) without downloading
//! any vCard body; bodies are then batch-fetched with
//! [`MultigetCards`](crate::rfc6352::card::multiget::MultigetCards).
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
//!     rfc6352::card::enumerate::EnumCards,
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
//!     EnumCards::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/contacts/");
//! let mut arg = None;
//!
//! let refs = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(refs)) => break refs,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} cards", refs.len());
//! ```

use alloc::{collections::BTreeSet, string::ToString};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{GETETAG, Property, ResponseEntry, WebdavAuth, report::Report, send::SendError},
    rfc6352::{addressbook::addressbook_query_body, card::types::CardRef},
    webdav_try,
};

const ENUM_PROPS: &[Property] = &[GETETAG];

/// Coroutine that enumerates card references (id plus ETag, no body)
/// inside an addressbook via REPORT `addressbook-query`.
#[derive(Debug)]
pub struct EnumCards {
    state: State,
}

impl EnumCards {
    /// Builds a new `enum-cards` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
    ) -> Self {
        let body = addressbook_query_body(ENUM_PROPS);
        let report = Report::new(base_url, auth, user_agent, addressbook_path, 1, body);
        Self {
            state: State::Report(report),
        }
    }
}

impl WebdavCoroutine for EnumCards {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<CardRef>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Report(report) => {
                let multistatus = webdav_try!(report, arg);
                let refs = multistatus
                    .responses
                    .iter()
                    .filter_map(from_entry)
                    .collect();
                WebdavCoroutineState::Complete(Ok(refs))
            }
        }
    }
}

fn from_entry(entry: &ResponseEntry) -> Option<CardRef> {
    let uri = entry.id();
    let id = uri.trim_end_matches(".vcf");
    if id.is_empty() {
        return None;
    }

    Some(CardRef {
        id: id.to_string(),
        uri: uri.to_string(),
        etag: entry
            .text(GETETAG)
            .map(|raw| raw.trim_matches('"').to_string()),
    })
}

#[derive(Debug)]
enum State {
    Report(Report),
}
