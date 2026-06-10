//! CardDAV card helpers shared across the read/create/update/delete
//! coroutines: resource path composition.

use alloc::{format, string::String};

/// Joins an addressbook collection path with a card id into the card
/// resource path (`.vcf` suffix included).
pub fn join_path(addressbook: &str, id: &str) -> String {
    let addressbook = addressbook.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{addressbook}/{id}.vcf")
}
