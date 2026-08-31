//! # Addressbook collections
//!
//! CardDAV addressbook collections (RFC 6352 §5).
//!
//! Holds the shared [`CarddavAddressbook`] type, the CardDAV property
//! vocabulary and the request-body helpers the addressbook coroutines reuse.
//! Each coroutine is its own submodule.

pub mod create;
pub mod delete;
pub mod home_set;
pub mod list;
pub mod update;

use alloc::{collections::BTreeSet, format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::rfc4918::{
    DISPLAYNAME, GETCTAG, RESOURCETYPE, SUPPORTED_REPORT_SET, SYNC_TOKEN, WebdavNamespace,
    WebdavPropValue, WebdavProperty, escape_text, report_query_body,
};

/// A CardDAV addressbook collection (RFC 6352 §5).
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct CarddavAddressbook {
    /// Addressbook identifier: the last non-empty path segment of the
    /// addressbook collection URL.
    pub id: String,
    /// Human-readable display name (DAV:displayname).
    pub display_name: Option<String>,
    /// Free-form description (RFC 6352 §6.2.1).
    pub description: Option<String>,
    /// Display color (custom inf-it.com extension, widely supported by CardDAV
    /// clients).
    pub color: Option<String>,
    /// Collection change tag (CalendarServer ctag extension), bumped on every
    /// change to the addressbook.
    pub ctag: Option<String>,
    /// Collection sync token (RFC 6578 §4), the checkpoint fed back to a
    /// `sync-collection` REPORT.
    pub sync_token: Option<String>,
    /// Reports the server advertises for this collection (RFC 3253 §3.1.5),
    /// e.g. `sync-collection`, `addressbook-query`, `addressbook-multiget`.
    ///
    /// RFC 6578 is an extension: a collection whose set holds no
    /// `sync-collection` is enumerated with
    /// [`WebdavSyncCollectionOptions::fallback`] instead, without paying a
    /// failed REPORT first.
    ///
    /// [`WebdavSyncCollectionOptions::fallback`]: crate::rfc6578::sync_collection::WebdavSyncCollectionOptions::fallback
    pub supported_reports: BTreeSet<String>,
}

/// A partial update over a [`CarddavAddressbook`]'s properties.
///
/// Each property is doubly optional, which a flat [`CarddavAddressbook`] cannot
/// express: [`None`] leaves the property alone, `Some(None)` removes it
/// (`DAV:remove`, RFC 4918 §9.2) and `Some(Some(value))` sets it (`DAV:set`).
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct CarddavAddressbookPatch {
    /// Identifier of the addressbook to patch: the last non-empty path segment
    /// of its collection URL.
    pub id: String,
    /// New human-readable display name (DAV:displayname).
    pub display_name: Option<Option<String>>,
    /// New free-form description (RFC 6352 §6.2.1).
    pub description: Option<Option<String>>,
    /// New display color (custom inf-it.com extension).
    pub color: Option<Option<String>>,
}

/// CardDAV namespace (RFC 6352 §4).
pub const CARDDAV: WebdavNamespace = WebdavNamespace {
    uri: "urn:ietf:params:xml:ns:carddav",
    prefix: "C",
};
/// inf-it extension namespace (addressbook color).
pub const INFIT: WebdavNamespace = WebdavNamespace {
    uri: "http://inf-it.com/ns/ab/",
    prefix: "I",
};

/// `C:addressbook` resourcetype marker (RFC 6352 §5.2).
pub const ADDRESSBOOK: WebdavProperty = WebdavProperty {
    ns: CARDDAV,
    local: "addressbook",
};
/// `C:addressbook-home-set` (RFC 6352 §7.1.1).
pub const ADDRESSBOOK_HOME_SET: WebdavProperty = WebdavProperty {
    ns: CARDDAV,
    local: "addressbook-home-set",
};
/// `C:addressbook-description` (RFC 6352 §6.2.1).
pub const ADDRESSBOOK_DESCRIPTION: WebdavProperty = WebdavProperty {
    ns: CARDDAV,
    local: "addressbook-description",
};
/// `C:address-data` (RFC 6352 §10.4).
pub const ADDRESS_DATA: WebdavProperty = WebdavProperty {
    ns: CARDDAV,
    local: "address-data",
};
/// `I:addressbook-color` (inf-it extension).
pub const ADDRESSBOOK_COLOR: WebdavProperty = WebdavProperty {
    ns: INFIT,
    local: "addressbook-color",
};
/// `C:addressbook-query` REPORT root (RFC 6352 §8.6).
pub const ADDRESSBOOK_QUERY: WebdavProperty = WebdavProperty {
    ns: CARDDAV,
    local: "addressbook-query",
};
/// `C:addressbook-multiget` REPORT root (RFC 6352 §8.7).
pub const ADDRESSBOOK_MULTIGET: WebdavProperty = WebdavProperty {
    ns: CARDDAV,
    local: "addressbook-multiget",
};

/// Properties requested when listing addressbooks.
pub const LIST_PROPS: &[WebdavProperty] = &[
    RESOURCETYPE,
    DISPLAYNAME,
    ADDRESSBOOK_DESCRIPTION,
    ADDRESSBOOK_COLOR,
    GETCTAG,
    SYNC_TOKEN,
    SUPPORTED_REPORT_SET,
];

/// Joins a home-set path with an addressbook id into a collection path
/// (trailing slash included).
pub fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}

/// Turns the display name, color and description of `addressbook` into `MKCOL`
/// set pairs.
pub fn property_set(
    addressbook: &CarddavAddressbook,
) -> Vec<(WebdavProperty, WebdavPropValue<'_>)> {
    let mut set = Vec::new();
    if let Some(name) = &addressbook.display_name {
        set.push((DISPLAYNAME, WebdavPropValue::Text(name)));
    }
    if let Some(color) = &addressbook.color {
        set.push((ADDRESSBOOK_COLOR, WebdavPropValue::Text(color)));
    }
    if let Some(description) = &addressbook.description {
        set.push((ADDRESSBOOK_DESCRIPTION, WebdavPropValue::Text(description)));
    }
    set
}

/// Splits `patch` into the `PROPPATCH` set pairs and the properties to remove.
pub fn property_updates(
    patch: &CarddavAddressbookPatch,
) -> (
    Vec<(WebdavProperty, WebdavPropValue<'_>)>,
    Vec<WebdavProperty>,
) {
    let fields = [
        (DISPLAYNAME, &patch.display_name),
        (ADDRESSBOOK_COLOR, &patch.color),
        (ADDRESSBOOK_DESCRIPTION, &patch.description),
    ];

    let mut set = Vec::new();
    let mut remove = Vec::new();

    for (property, field) in fields {
        match field {
            None => continue,
            Some(None) => remove.push(property),
            Some(Some(value)) => set.push((property, WebdavPropValue::Text(value))),
        }
    }

    (set, remove)
}

/// Builds a CardDAV `addressbook-query` REPORT body requesting `props`, with a
/// match-all filter.
///
/// RFC 6352 §8.6 requires `C:filter`, and an empty `allof` matches every
/// card, an empty conjunction being true. Strict servers (Google) 400 a
/// missing filter and read an empty `anyof` as matching nothing.
pub fn addressbook_query_body(props: &[WebdavProperty]) -> Vec<u8> {
    let filter = "<C:filter test=\"allof\"></C:filter>";
    report_query_body(ADDRESSBOOK_QUERY, &[CARDDAV], props, filter)
}

/// Builds a CardDAV `addressbook-multiget` REPORT body (RFC 6352 §8.7)
/// requesting `props` for each given href.
pub fn addressbook_multiget_body(hrefs: &[String], props: &[WebdavProperty]) -> Vec<u8> {
    let mut fragment = String::new();
    for href in hrefs {
        fragment.push_str(&format!("<D:href>{}</D:href>", escape_text(href)));
    }
    report_query_body(ADDRESSBOOK_MULTIGET, &[CARDDAV], props, &fragment)
}
