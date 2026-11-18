//! # Calendar items
//!
//! CalDAV calendar object resources, a.k.a. items (RFC 4791 §4.1).
//!
//! Holds the [`CaldavItemRef`] and [`CaldavItemEntry`] types shared across the
//! item coroutines, plus the item-property selector, the resource path
//! composition and the multistatus entry mapper. Each coroutine is its own
//! submodule, together with the result types only it returns.

#[cfg(feature = "client")]
pub mod create;
#[cfg(feature = "client")]
pub mod delete;
#[cfg(feature = "client")]
pub mod enumerate;
#[cfg(feature = "client")]
pub mod list;
#[cfg(feature = "client")]
pub mod r#move;
#[cfg(feature = "client")]
pub mod multiget;
#[cfg(feature = "client")]
pub mod read;
#[cfg(feature = "client")]
pub mod update;

use alloc::{format, string::String, vec::Vec};

#[cfg(feature = "client")]
use serde::Deserialize;

use crate::rfc4918::encode_segments;
#[cfg(feature = "client")]
use crate::{
    rfc4791::calendar::CALENDAR_DATA,
    rfc4918::{
        GETETAG, WebdavProperty,
        typed::{WebdavResponseOf, WebdavUnreadableEntry, whole_text},
        unquoted_etag,
    },
};

/// Item reference (id plus ETag, no body) returned by
/// [`CaldavItemEnum`](crate::rfc4791::item::enumerate::CaldavItemEnum).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CaldavItemRef {
    /// Resource id: the last path segment of the item href, read as
    /// [`id_from_location`](crate::rfc4918::id_from_location) reads one, and
    /// the addressing key of every other verb.
    pub id: String,
    /// The `<href>` as served, percent-encoding kept: the key for a store
    /// keeping the server's spelling, which [`id`](Self::id) may decode.
    pub href: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Raw calendar item entry returned by
/// [`CaldavItemList`](crate::rfc4791::item::list::CaldavItemList).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CaldavItemEntry {
    /// Resource id: the last path segment of the item's href (see
    /// [`CaldavItemRef::id`]).
    pub id: String,
    /// The `<href>` as served (see [`CaldavItemRef::href`]).
    pub href: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
    /// Raw iCalendar bytes: the whole text of `calendar-data`, untrimmed.
    pub data: Vec<u8>,
}

/// Properties requested when listing or batch-fetching item bodies.
#[cfg(feature = "client")]
pub(crate) const ITEM_PROPS: &[WebdavProperty] = &[GETETAG, CALENDAR_DATA];

/// Joins a calendar collection path with an item resource id into the item
/// resource path. io-webdav never adds nor strips an extension.
///
/// Each byte a path segment cannot hold raw is percent-encoded (RFC 3986
/// §3.3), an escape already there kept, so an id read from a listing
/// addresses the segment it came from.
/// [`check_resource_id`](crate::rfc4918::check_resource_id) refuses an id
/// addressing another resource.
pub fn join_path(calendar: &str, id: &str) -> String {
    let calendar = calendar.trim_end_matches('/');
    let id = encode_segments(id.trim_start_matches('/'));
    format!("{calendar}/{id}")
}

/// The properties of [`ITEM_PROPS`], as a typed multistatus reads them.
#[cfg(feature = "client")]
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct CaldavItemProps {
    pub getetag: Option<String>,
    #[serde(default, deserialize_with = "whole_text")]
    pub calendar_data: Option<String>,
}

/// Maps a multistatus response carrying [`ITEM_PROPS`] to a
/// [`CaldavItemEntry`] (id, etag, raw iCalendar bytes), or to the
/// [`WebdavUnreadableEntry`] of an item that came without its data.
///
/// Answers [`None`] for a response naming no item: a self-entry, an href
/// without a last segment, or a 507 truncation row.
#[cfg(feature = "client")]
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

    let id = id.into_owned();
    let data = response
        .props
        .calendar_data
        .filter(|data| !data.trim().is_empty());

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
        href: response.href,
        etag: unquoted_etag(response.props.getetag),
        data: data.into_bytes(),
    }))
}
