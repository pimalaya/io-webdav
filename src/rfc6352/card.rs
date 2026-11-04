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

use serde::Deserialize;

use crate::{
    rfc4918::{GETETAG, WebdavProperty, typed::WebdavResponseOf, unquoted_etag},
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

/// The properties of [`CARD_PROPS`], as a typed multistatus reads them.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct CarddavCardProps {
    pub getetag: Option<String>,
    pub address_data: Option<String>,
}

/// Maps a multistatus response carrying [`CARD_PROPS`] to a
/// [`CarddavCardEntry`] (id, etag, raw vCard bytes).
pub(crate) fn card_from_response(
    response: WebdavResponseOf<CarddavCardProps>,
) -> Option<CarddavCardEntry> {
    // NOTE: a self-entry (its href ends in a slash) is never a card; iCloud
    // echoes the addressbook itself in the multistatus.
    if response.href.ends_with('/') {
        return None;
    }

    let id = response.id();
    if id.is_empty() {
        return None;
    }

    let id = id.to_string();
    let data = response
        .props
        .address_data
        .filter(|data| !data.is_empty())?;

    Some(CarddavCardEntry {
        id,
        etag: unquoted_etag(response.props.getetag),
        data: data.into_bytes(),
    })
}
