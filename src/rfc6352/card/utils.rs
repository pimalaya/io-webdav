//! CardDAV card helpers shared across the card coroutines: resource
//! path composition and the multistatus entry mapper.

use alloc::{format, string::String, string::ToString};

use crate::{
    rfc4918::{GETETAG, Property, ResponseEntry, trace_unrecognized},
    rfc6352::{addressbook::ADDRESS_DATA, card::types::CardEntry},
};

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
