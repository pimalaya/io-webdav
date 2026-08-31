//! # Cards
//!
//! CardDAV address object resources, a.k.a. cards (RFC 6352 §5.1).
//!
//! Holds the [`CarddavCardRef`] and [`CarddavCardEntry`] types shared across
//! the card coroutines, plus the card-property selector, the resource path
//! composition and the multistatus entry mapper.
//!
//! Each coroutine is its own submodule, together with the result types only
//! it returns.

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
    rfc4918::{GETETAG, WebdavProperty, WebdavResponseEntry, trace_unrecognized},
    rfc6352::addressbook::ADDRESS_DATA,
};

/// Card reference (id plus ETag, no body) returned by
/// [`CarddavCardEnum`](crate::rfc6352::card::enumerate::CarddavCardEnum).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CarddavCardRef {
    /// Resource id: the last path segment of the card href, exactly as the
    /// server returned it, and the addressing key of every other verb.
    pub id: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Raw card entry returned by
/// [`CarddavCardList`](crate::rfc6352::card::list::CarddavCardList).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CarddavCardEntry {
    /// Resource id: the last path segment of the card's href, exactly as the
    /// server returned it (see [`CarddavCardRef::id`]).
    pub id: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
    /// Raw vCard bytes (`address-data`).
    pub data: Vec<u8>,
}

/// Properties requested when listing or batch-fetching card bodies.
pub(crate) const CARD_PROPS: &[WebdavProperty] = &[GETETAG, ADDRESS_DATA];

/// Joins an addressbook collection path with a card resource id, used verbatim,
/// into the card resource path. io-webdav never adds nor strips an extension.
pub fn join_path(addressbook: &str, id: &str) -> String {
    let addressbook = addressbook.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{addressbook}/{id}")
}

/// Maps a multistatus response entry carrying [`CARD_PROPS`] to a
/// [`CarddavCardEntry`] (id, etag, raw vCard bytes).
pub(crate) fn card_from_entry(entry: &WebdavResponseEntry) -> Option<CarddavCardEntry> {
    // NOTE: a self-entry (its href ends in a slash) is never a card; iCloud
    // echoes the addressbook itself in the multistatus.
    if entry.href.ends_with('/') {
        return None;
    }

    let id = entry.id();
    if id.is_empty() {
        return None;
    }

    let data = entry.text(ADDRESS_DATA)?;
    trace_unrecognized(entry, CARD_PROPS);

    Some(CarddavCardEntry {
        id: id.to_string(),
        etag: entry
            .text(GETETAG)
            .map(|raw| raw.trim_matches('"').to_string()),
        data: data.as_bytes().to_vec(),
    })
}
