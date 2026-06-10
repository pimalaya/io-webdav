//! Shared CalDAV item types returned by the list/read/create/update
//! coroutines.

use alloc::{string::String, vec::Vec};

/// Raw calendar item entry returned by
/// [`ListItems`](crate::rfc4791::item::list::ListItems).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemEntry {
    /// Item identifier (last path segment of the href, with `.ics`
    /// stripped).
    pub id: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,

    /// Raw iCalendar bytes (`calendar-data`).
    pub data: Vec<u8>,
}

/// Item body plus optional ETag returned by
/// [`ReadItem`](crate::rfc4791::item::read::ReadItem).
#[derive(Clone, Debug)]
pub struct ItemBody {
    /// Raw iCalendar bytes.
    pub data: Vec<u8>,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Outcome of a successful
/// [`CreateItem`](crate::rfc4791::item::create::CreateItem) resume.
#[derive(Clone, Debug)]
pub struct CreateItemOk {
    /// Item identifier (as supplied by the caller).
    pub id: String,
    /// Entity tag returned by the server, when present.
    pub etag: Option<String>,
}

/// Outcome of a successful
/// [`UpdateItem`](crate::rfc4791::item::update::UpdateItem) resume.
#[derive(Clone, Debug)]
pub struct UpdateItemOk {
    /// Item identifier (as supplied by the caller).
    pub id: String,
    /// Updated entity tag returned by the server, when present.
    pub etag: Option<String>,
}
