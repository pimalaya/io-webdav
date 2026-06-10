//! CalDAV item helpers shared across the read/create/update/delete
//! coroutines: resource path composition.

use alloc::{format, string::String};

/// Joins a calendar collection path with an item id into the item
/// resource path (`.ics` suffix included).
pub fn join_path(calendar: &str, id: &str) -> String {
    let calendar = calendar.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{calendar}/{id}.ics")
}
