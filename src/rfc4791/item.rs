//! CalDAV calendar object resources, a.k.a. items (RFC 4791 §4.1).
//!
//! Holds the [`CaldavItemRef`] and [`CaldavItemEntry`] types shared across the item
//! coroutines, plus the crate-internal item-property selector, resource
//! path composition and multistatus entry mapper. Each coroutine
//! (create, delete, enumerate, list, multiget, read, update) is its own
//! submodule, and the single-coroutine result types live there.

pub mod create;
pub mod delete;
pub mod enumerate;
pub mod list;
pub mod multiget;
pub mod read;
pub mod update;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    rfc4791::calendar::CALENDAR_DATA,
    rfc4918::{GETETAG, WebdavProperty, WebdavResponseEntry, trace_unrecognized},
};

/// Item reference (id plus ETag, no body) returned by
/// [`CaldavItemEnum`](crate::rfc4791::item::enumerate::CaldavItemEnum).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CaldavItemRef {
    /// Resource id: the item's href last path segment, exactly as the
    /// server returned it, and the addressing key for read/update/delete.
    /// io-webdav never adds nor strips a file extension.
    pub id: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Raw calendar item entry returned by
/// [`CaldavItemList`](crate::rfc4791::item::list::CaldavItemList).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CaldavItemEntry {
    /// Resource id: the last path segment of the item's href, exactly
    /// as the server returned it (see [`CaldavItemRef::id`]).
    pub id: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
    /// Raw iCalendar bytes (`calendar-data`).
    pub data: Vec<u8>,
}

/// Properties requested when listing or batch-fetching item bodies.
pub(crate) const ITEM_PROPS: &[WebdavProperty] = &[GETETAG, CALENDAR_DATA];

/// Joins a calendar collection path with an item resource id (an
/// `CaldavItemEntry::id` / `CaldavItemRef::id`, used verbatim) into the item
/// resource path. io-webdav never adds nor strips a file extension.
pub fn join_path(calendar: &str, id: &str) -> String {
    let calendar = calendar.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{calendar}/{id}")
}

/// Maps a multistatus response entry carrying [`ITEM_PROPS`] to an
/// [`CaldavItemEntry`] (id, etag, raw iCalendar bytes).
pub(crate) fn item_from_entry(entry: &WebdavResponseEntry) -> Option<CaldavItemEntry> {
    // A collection self-entry (its href ends in a slash) is never an
    // item; iCloud echoes the calendar itself in the multistatus.
    if entry.href.ends_with('/') {
        return None;
    }

    let id = entry.id();
    if id.is_empty() {
        return None;
    }

    let data = entry.text(CALENDAR_DATA)?;
    trace_unrecognized(entry, ITEM_PROPS);

    Some(CaldavItemEntry {
        id: id.to_string(),
        etag: entry
            .text(GETETAG)
            .map(|raw| raw.trim_matches('"').to_string()),
        data: data.as_bytes().to_vec(),
    })
}
