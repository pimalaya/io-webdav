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

use alloc::{format, string::String, vec::Vec};

use serde::Deserialize;

use crate::{
    rfc4918::{
        GETETAG, WebdavProperty, encode_segments,
        typed::{WebdavResponseOf, WebdavUnreadableEntry, whole_text},
        unquoted_etag,
    },
    rfc6352::addressbook::ADDRESS_DATA,
};

/// Card reference (id plus ETag, no body) returned by
/// [`CarddavCardEnum`](crate::rfc6352::card::enumerate::CarddavCardEnum).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CarddavCardRef {
    /// Resource id: the last path segment of the card href, read as
    /// [`id_from_location`](crate::rfc4918::id_from_location) reads one, and
    /// the addressing key of every other verb.
    pub id: String,
    /// The `<href>` as served, percent-encoding kept: the key for a store
    /// keeping the server's spelling, which [`id`](Self::id) may decode.
    pub href: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Raw card entry returned by
/// [`CarddavCardList`](crate::rfc6352::card::list::CarddavCardList).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CarddavCardEntry {
    /// Resource id: the last path segment of the card's href (see
    /// [`CarddavCardRef::id`]).
    pub id: String,
    /// The `<href>` as served (see [`CarddavCardRef::href`]).
    pub href: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
    /// Raw vCard bytes: the whole text of `address-data`, untrimmed.
    pub data: Vec<u8>,
}

/// Properties requested when listing or batch-fetching card bodies.
pub(crate) const CARD_PROPS: &[WebdavProperty] = &[GETETAG, ADDRESS_DATA];

/// Joins an addressbook collection path with a card resource id into the card
/// resource path. io-webdav never adds nor strips an extension.
///
/// Each byte a path segment cannot hold raw is percent-encoded (RFC 3986
/// §3.3), an escape already there kept, so an id read from a listing
/// addresses the segment it came from.
/// [`check_resource_id`](crate::rfc4918::check_resource_id) refuses an id
/// addressing another resource.
pub fn join_path(addressbook: &str, id: &str) -> String {
    let addressbook = addressbook.trim_end_matches('/');
    let id = encode_segments(id.trim_start_matches('/'));
    format!("{addressbook}/{id}")
}

/// The properties of [`CARD_PROPS`], as a typed multistatus reads them.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct CarddavCardProps {
    pub getetag: Option<String>,
    #[serde(default, deserialize_with = "whole_text")]
    pub address_data: Option<String>,
}

/// Maps a multistatus response carrying [`CARD_PROPS`] to a
/// [`CarddavCardEntry`] (id, etag, raw vCard bytes), or to the
/// [`WebdavUnreadableEntry`] of a card that came without its data.
///
/// Answers [`None`] for a response naming no card: a self-entry, an href
/// without a last segment, or a 507 truncation row.
pub(crate) fn card_from_response(
    response: WebdavResponseOf<CarddavCardProps>,
) -> Option<Result<CarddavCardEntry, WebdavUnreadableEntry>> {
    // NOTE: a self-entry (its href ends in a slash) is never a card; iCloud
    // echoes the addressbook itself in the multistatus.
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
        .address_data
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

    Some(Ok(CarddavCardEntry {
        id,
        href: response.href,
        etag: unquoted_etag(response.props.getetag),
        data: data.into_bytes(),
    }))
}
