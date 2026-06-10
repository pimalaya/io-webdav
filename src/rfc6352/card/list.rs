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

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        request::WebdavRequest,
        send::{Send, SendError, SendOk},
        {Multistatus, Value, WebdavAuth},
    },
    rfc6352::card::types::CardEntry,
    webdav_try,
};

const BODY: &str = include_str!("./list.xml");

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
        let request = WebdavRequest::report(base_url, auth, user_agent, addressbook_path)
            .content_type_xml()
            .depth(1)
            .body(BODY.as_bytes().to_vec());

        Self {
            state: State::Send(Send::new(request)),
        }
    }
}

impl WebdavCoroutine for ListCards {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<CardEntry>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("list-cards: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                WebdavCoroutineState::Complete(Ok(collect(&ok)))
            }
        }
    }
}

fn collect(ok: &SendOk<Multistatus<Prop>>) -> BTreeSet<CardEntry> {
    let mut cards = BTreeSet::new();

    let Some(responses) = &ok.body.responses else {
        return cards;
    };

    for response in responses {
        trace!("process multistatus response");

        if let Some(status) = &response.status {
            if !status.is_success() {
                trace!("skip multistatus response with non-2xx status");
                continue;
            }
        }

        let Some(propstats) = &response.propstats else {
            continue;
        };

        let id = response
            .href
            .value
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".vcf")
            .to_string();

        if id.is_empty() {
            continue;
        }

        for propstat in propstats {
            if !propstat.status.is_success() {
                trace!("skip propstat with non-2xx status");
                continue;
            }

            let Some(data) = &propstat.prop.address_data else {
                continue;
            };

            let etag = propstat
                .prop
                .getetag
                .as_deref()
                .map(|raw| raw.trim_matches('"').to_string());

            cards.insert(CardEntry {
                id: id.clone(),
                etag,
                data: data.value.as_bytes().to_vec(),
            });

            break;
        }
    }

    cards
}

#[derive(Debug)]
enum State {
    Send(Send<Multistatus<Prop>>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

/// `<prop>` payload returned by the list-cards REPORT.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub getetag: Option<String>,
    pub address_data: Option<Value>,
}
