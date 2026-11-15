//! # Calendar items
//!
//! CalDAV calendar object resources, a.k.a. items (RFC 4791 §4.1).
//!
//! Holds the [`CaldavItemRef`] and [`CaldavItemEntry`] types shared across the
//! item coroutines, plus the item-property selector, the resource path
//! composition and the multistatus entry mapper. Each coroutine is its own
//! submodule, together with the result types only it returns.

pub mod create;
pub mod delete;
pub mod enumerate;
pub mod list;
pub mod r#move;
pub mod multiget;
pub mod read;
pub mod update;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::Deserialize;

use crate::{
    rfc4791::calendar::CALENDAR_DATA,
    rfc4918::{
        GETETAG, WebdavProperty,
        typed::{WebdavResponseOf, WebdavUnreadableEntry},
        unquoted_etag,
    },
};

/// Item reference (id plus ETag, no body) returned by
/// [`CaldavItemEnum`](crate::rfc4791::item::enumerate::CaldavItemEnum).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CaldavItemRef {
    /// Resource id: the last path segment of the item href, exactly as the
    /// server returned it, and the addressing key of every other verb.
    pub id: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Raw calendar item entry returned by
/// [`CaldavItemList`](crate::rfc4791::item::list::CaldavItemList).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CaldavItemEntry {
    /// Resource id: the last path segment of the item's href, exactly as the
    /// server returned it (see [`CaldavItemRef::id`]).
    pub id: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
    /// Raw iCalendar bytes (`calendar-data`).
    pub data: Vec<u8>,
}

/// Properties requested when listing or batch-fetching item bodies.
pub(crate) const ITEM_PROPS: &[WebdavProperty] = &[GETETAG, CALENDAR_DATA];

/// Joins a calendar collection path with an item resource id, used verbatim,
/// into the item resource path. io-webdav never adds nor strips an extension.
///
/// The id is joined as given;
/// [`check_resource_id`](crate::rfc4918::check_resource_id) refuses one
/// addressing another resource.
pub fn join_path(calendar: &str, id: &str) -> String {
    let calendar = calendar.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{calendar}/{id}")
}

/// The properties of [`ITEM_PROPS`], as a typed multistatus reads them.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct CaldavItemProps {
    pub getetag: Option<String>,
    pub calendar_data: Option<String>,
}

/// Maps a multistatus response carrying [`ITEM_PROPS`] to a
/// [`CaldavItemEntry`] (id, etag, raw iCalendar bytes), or to the
/// [`WebdavUnreadableEntry`] of an item that came without its data.
///
/// Answers [`None`] for a response naming no item: a self-entry, an href
/// without a last segment, or a 507 truncation row.
pub(crate) fn item_from_response(
    response: WebdavResponseOf<CaldavItemProps>,
) -> Option<Result<CaldavItemEntry, WebdavUnreadableEntry>> {
    // NOTE: a self-entry (its href ends in a slash) is never an item; iCloud
    // echoes the calendar itself in the multistatus.
    if response.href.ends_with('/') {
        return None;
    }

    let id = response.id();
    if id.is_empty() {
        return None;
    }

    let id = id.to_string();
    let data = response.props.calendar_data.filter(|data| !data.is_empty());

    let Some(data) = data else {
        if response.status == Some(507) {
            return None;
        }

        return Some(Err(WebdavUnreadableEntry {
            id,
            href: response.href,
            status: response.status,
            failures: response.failures,
        }));
    };

    Some(Ok(CaldavItemEntry {
        id,
        etag: unquoted_etag(response.props.getetag),
        data: data.into_bytes(),
    }))
}
