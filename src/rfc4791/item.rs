//! CalDAV calendar object resources, a.k.a. items (RFC 4791 §4.1).
//!
//! The single-coroutine result types live in their own coroutine
//! submodules; this module carries only the resource-path helper shared
//! across them. Each coroutine (create, delete, list, read, update) is
//! its own submodule.

pub mod create;
pub mod delete;
pub mod list;
pub mod read;
pub mod update;

use alloc::{format, string::String};

/// Joins a calendar collection path with an item id into the item
/// resource path (`.ics` suffix included).
pub fn join_path(calendar: &str, id: &str) -> String {
    let calendar = calendar.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{calendar}/{id}.ics")
}
