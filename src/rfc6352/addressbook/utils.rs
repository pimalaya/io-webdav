//! CardDAV addressbook vocabulary (RFC 6352 §5, §6) plus the
//! request-body helpers shared across the addressbook/card coroutines.

use alloc::{format, string::String, vec::Vec};

use crate::{
    rfc4918::{DISPLAYNAME, Namespace, Property, RESOURCETYPE, report_query_body},
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

/// Properties requested when listing addressbooks.
pub const LIST_PROPS: &[Property] = &[
    RESOURCETYPE,
    DISPLAYNAME,
    ADDRESSBOOK_DESCRIPTION,
    ADDRESSBOOK_COLOR,
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

/// Builds a CardDAV `addressbook-query` REPORT body requesting `props`.
pub fn addressbook_query_body(props: &[Property]) -> Vec<u8> {
    report_query_body(ADDRESSBOOK_QUERY, &[CARDDAV], props, "")
}
