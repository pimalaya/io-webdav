//! `list-items` coroutine: REPORT `calendar-query` against a calendar
//! collection.
//!
//! Stays byte-oriented: the iCalendar payload is returned as raw bytes
//! and parsed by io-calendar.
//!
//! Lifted from io-calendar/src/caldav/coroutines/list-items.rs.

use alloc::{
    collections::BTreeSet,
    format,
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

const BODY: &str = include_str!("./list_items.xml");

/// Raw calendar item entry returned by [`ListItems`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemEntry {
    /// Item identifier (last path segment of the href, with `.ics`
    /// stripped).
    pub id: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,

    /// Raw iCalendar bytes (`calendar-data`).
    pub data: Vec<u8>,
}

/// Coroutine that lists items inside a calendar via REPORT
/// `calendar-query`.
#[derive(Debug)]
pub struct ListItems(Send<Multistatus<Prop>>);

impl ListItems {
    /// Builds a new `list-items` coroutine.
    ///
    /// `calendar_path` is the calendar collection path. `comp_filter`
    /// is the optional VCALENDAR child filter (e.g.
    /// `<C:comp-filter name="VEVENT" />`); pass an empty string to
    /// list every component type.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        comp_filter: &str,
    ) -> Self {
        let body = format!("{}", BODY).replacen("{}", comp_filter, 1).into_bytes();

        let request = WebdavRequest::report(base_url, auth, user_agent, calendar_path)
            .content_type_xml()
            .depth(1)
            .body(body);

        Self(Send::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<BTreeSet<ItemEntry>> {
        match self.0.resume(arg) {
            SendResult::Ok(ok) => {
                let items = collect(&ok);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: items,
                })
            }
            SendResult::WantsRead => SendResult::WantsRead,
            SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            SendResult::Err(err) => SendResult::Err(err),
        }
    }
}

fn collect(ok: &SendOk<Multistatus<Prop>>) -> BTreeSet<ItemEntry> {
    let mut items = BTreeSet::new();

    let Some(responses) = &ok.body.responses else {
        return items;
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
            .trim_end_matches(".ics")
            .to_string();

        if id.is_empty() {
            continue;
        }

        for propstat in propstats {
            if !propstat.status.is_success() {
                trace!("skip propstat with non-2xx status");
                continue;
            }

            let Some(data) = &propstat.prop.calendar_data else {
                continue;
            };

            let etag = propstat
                .prop
                .getetag
                .as_deref()
                .map(|raw| raw.trim_matches('"').to_string());

            items.insert(ItemEntry {
                id: id.clone(),
                etag,
                data: data.value.as_bytes().to_vec(),
            });

            break;
        }
    }

    items
}

/// `<prop>` payload returned by the list-items REPORT.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub getetag: Option<String>,
    pub calendar_data: Option<Value>,
}
