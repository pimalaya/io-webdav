//! CardDAV address object resources, a.k.a. cards (RFC 6352 §5.1).
//!
//! Holds the [`CardRef`] and [`CardEntry`] types shared across the card
//! coroutines, plus the crate-internal card-property selector, resource
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
    rfc4918::{GETETAG, Property, ResponseEntry, trace_unrecognized},
    rfc6352::addressbook::ADDRESS_DATA,
};

/// Card reference (id plus ETag, no body) returned by
/// [`EnumCards`](crate::rfc6352::card::enumerate::EnumCards).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CardRef {
    /// Display identifier: [`uri`](Self::uri) with any `.vcf` stripped.
    pub id: String,

    /// Resource name (last path segment of the href), exactly as the
    /// server returned it; the addressing key of the read, update and
    /// delete coroutines. Servers are not required to suffix `.vcf`.
    pub uri: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Raw card entry returned by
/// [`ListCards`](crate::rfc6352::card::list::ListCards).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CardEntry {
    /// Display identifier: [`uri`](Self::uri) with any `.vcf` stripped.
    pub id: String,

    /// Resource name (last path segment of the href), exactly as the
    /// server returned it; the addressing key of the read, update and
    /// delete coroutines. Servers are not required to suffix `.vcf`.
    pub uri: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,

    /// Raw vCard bytes (`address-data`).
    pub data: Vec<u8>,
}

/// Properties requested when listing or batch-fetching card bodies.
pub(crate) const CARD_PROPS: &[Property] = &[GETETAG, ADDRESS_DATA];

/// Joins an addressbook collection path with a card resource name into
/// the card resource path. The name is used verbatim: for existing
/// cards it must be the server's own (`CardEntry::uri` / `CardRef::uri`,
/// not the display id), since servers are not required to suffix
/// `.vcf`; only creation appends the extension, in `CreateCard`.
pub fn join_path(addressbook: &str, uri: &str) -> String {
    let addressbook = addressbook.trim_end_matches('/');
    let uri = uri.trim_start_matches('/');
    format!("{addressbook}/{uri}")
}

/// Maps a multistatus response entry carrying [`CARD_PROPS`] to a
/// [`CardEntry`] (id, uri, etag, raw vCard bytes).
pub(crate) fn card_from_entry(entry: &ResponseEntry) -> Option<CardEntry> {
    // A collection self-entry (its href ends in a slash) is never a
    // card; iCloud echoes the addressbook itself in the multistatus.
    if entry.href.ends_with('/') {
        return None;
    }

    let uri = entry.id();
    let id = uri.trim_end_matches(".vcf");
    if id.is_empty() {
        return None;
    }

    let data = entry.text(ADDRESS_DATA)?;
    trace_unrecognized(entry, CARD_PROPS);

    Some(CardEntry {
        id: id.to_string(),
        uri: uri.to_string(),
        etag: entry
            .text(GETETAG)
            .map(|raw| raw.trim_matches('"').to_string()),
        data: data.as_bytes().to_vec(),
    })
}
