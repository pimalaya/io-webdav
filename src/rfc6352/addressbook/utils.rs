//! CardDAV addressbook helpers shared across the create/update/delete
//! coroutines: collection path composition and request-body templating.

use alloc::{format, string::String};

use crate::rfc6352::addressbook::types::Addressbook;

/// Joins a home-set path with an addressbook id into a collection path
/// (trailing slash included, RFC 6352 §5).
pub fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}

/// Fills a `MKCOL` / `PROPPATCH` body `template` with the
/// addressbook's display name, color and description fragments. Each
/// `{}` placeholder is replaced once, in that order.
pub fn format_body(template: &str, addressbook: &Addressbook) -> String {
    let name = match &addressbook.display_name {
        Some(value) => format!("<displayname>{value}</displayname>"),
        None => String::new(),
    };

    let color = match &addressbook.color {
        Some(value) => format!("<I:addressbook-color>{value}</I:addressbook-color>"),
        None => String::new(),
    };

    let description = match &addressbook.description {
        Some(value) => format!("<C:addressbook-description>{value}</C:addressbook-description>"),
        None => String::new(),
    };

    template
        .replacen("{}", &name, 1)
        .replacen("{}", &color, 1)
        .replacen("{}", &description, 1)
}
