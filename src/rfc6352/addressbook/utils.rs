//! CardDAV addressbook vocabulary (RFC 6352 §5, §6) plus the
//! request-body helpers shared across the addressbook/card coroutines.

use alloc::{format, string::String, vec::Vec};

use crate::{
    rfc4918::{
        DISPLAYNAME, GETCTAG, Namespace, Property, RESOURCETYPE, SYNC_TOKEN, escape_text,
        report_query_body,
    },
    rfc6352::addressbook::types::Addressbook,
};

/// CardDAV namespace (RFC 6352 §4).
pub const CARDDAV: Namespace = Namespace {
    uri: "urn:ietf:params:xml:ns:carddav",
    prefix: "C",
};
/// inf-it extension namespace (addressbook color).
pub const INFIT: Namespace = Namespace {
    uri: "http://inf-it.com/ns/ab/",
    prefix: "I",
};

/// `C:addressbook` resourcetype marker (RFC 6352 §5.2).
pub const ADDRESSBOOK: Property = Property {
    ns: CARDDAV,
    local: "addressbook",
};
/// `C:addressbook-home-set` (RFC 6352 §7.1.1).
pub const ADDRESSBOOK_HOME_SET: Property = Property {
    ns: CARDDAV,
    local: "addressbook-home-set",
};
/// `C:addressbook-description` (RFC 6352 §6.2.1).
pub const ADDRESSBOOK_DESCRIPTION: Property = Property {
    ns: CARDDAV,
    local: "addressbook-description",
};
/// `C:address-data` (RFC 6352 §10.4).
pub const ADDRESS_DATA: Property = Property {
    ns: CARDDAV,
    local: "address-data",
};
/// `I:addressbook-color` (inf-it extension).
pub const ADDRESSBOOK_COLOR: Property = Property {
    ns: INFIT,
    local: "addressbook-color",
};
/// `C:addressbook-query` REPORT root (RFC 6352 §8.6).
pub const ADDRESSBOOK_QUERY: Property = Property {
    ns: CARDDAV,
    local: "addressbook-query",
};
/// `C:addressbook-multiget` REPORT root (RFC 6352 §8.7).
pub const ADDRESSBOOK_MULTIGET: Property = Property {
    ns: CARDDAV,
    local: "addressbook-multiget",
};

/// Properties requested when listing addressbooks.
pub const LIST_PROPS: &[Property] = &[
    RESOURCETYPE,
    DISPLAYNAME,
    ADDRESSBOOK_DESCRIPTION,
    ADDRESSBOOK_COLOR,
    GETCTAG,
    SYNC_TOKEN,
];

/// Joins a home-set path with an addressbook id into a collection path
/// (trailing slash included).
pub fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}

/// The present display name / color / description of `addressbook` as
/// `PROPPATCH` / `MKCOL` set pairs.
pub fn property_set(addressbook: &Addressbook) -> Vec<(Property, &str)> {
    let mut set = Vec::new();
    if let Some(name) = &addressbook.display_name {
        set.push((DISPLAYNAME, name.as_str()));
    }
    if let Some(color) = &addressbook.color {
        set.push((ADDRESSBOOK_COLOR, color.as_str()));
    }
    if let Some(description) = &addressbook.description {
        set.push((ADDRESSBOOK_DESCRIPTION, description.as_str()));
    }
    set
}

/// Builds a CardDAV `addressbook-query` REPORT body requesting `props`,
/// with a match-all filter.
///
/// RFC 6352 §8.6 requires the `C:filter` element; an empty `allof`
/// filter matches every card (an empty conjunction is true). Strict
/// servers (Google) reject a missing filter with HTTP 400 and treat an
/// empty `anyof` (the schema default) as matching nothing, so `allof`
/// is the portable match-all form.
pub fn addressbook_query_body(props: &[Property]) -> Vec<u8> {
    let filter = "<C:filter test=\"allof\"></C:filter>";
    report_query_body(ADDRESSBOOK_QUERY, &[CARDDAV], props, filter)
}

/// Builds a CardDAV `addressbook-multiget` REPORT body (RFC 6352 §8.7)
/// requesting `props` for each given href.
pub fn addressbook_multiget_body(hrefs: &[String], props: &[Property]) -> Vec<u8> {
    let mut fragment = String::new();
    for href in hrefs {
        fragment.push_str(&format!("<href>{}</href>", escape_text(href)));
    }
    report_query_body(ADDRESSBOOK_MULTIGET, &[CARDDAV], props, &fragment)
}
