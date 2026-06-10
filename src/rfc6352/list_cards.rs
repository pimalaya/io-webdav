//! `list-cards` coroutine: REPORT `addressbook-query` against an
//! addressbook collection.
//!
//! Stays byte-oriented: the vCard payload is returned as raw bytes
//! and parsed by io-addressbook.
//!
//! Lifted from io-addressbook/src/carddav/coroutines/list-cards.rs.

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    request::WebdavRequest,
    response::{Multistatus, Value},
    send::{Send, SendOk, SendResult},
};

const BODY: &str = include_str!("./list_cards.xml");

/// Raw card entry returned by [`ListCards`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CardEntry {
    /// Card identifier (last path segment of the href, with `.vcf`
    /// stripped).
    pub id: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,

    /// Raw vCard bytes (`address-data`).
    pub data: Vec<u8>,
}

/// Coroutine that lists cards inside an addressbook via REPORT
/// `addressbook-query`.
#[derive(Debug)]
pub struct ListCards(Send<Multistatus<Prop>>);

impl ListCards {
    /// Builds a new `list-cards` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, addressbook_path: &str) -> Self {
        let request = WebdavRequest::report(base_url, auth, user_agent, addressbook_path)
            .content_type_xml()
            .depth(1)
            .body(BODY.as_bytes().to_vec());

        Self(Send::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<BTreeSet<CardEntry>> {
        match self.0.resume(arg) {
            SendResult::Ok(ok) => {
                let cards = collect(&ok);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: cards,
                })
            }
            SendResult::WantsRead => SendResult::WantsRead,
            SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            SendResult::Err(err) => SendResult::Err(err),
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

/// `<prop>` payload returned by the list-cards REPORT.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub getetag: Option<String>,
    pub address_data: Option<Value>,
}
