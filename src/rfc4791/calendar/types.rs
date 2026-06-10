//! Shared `Calendar` type returned by CalDAV list/create coroutines.

use alloc::string::String;

use serde::{Deserialize, Serialize};

/// A CalDAV calendar collection (RFC 4791 §4).
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Calendar {
    /// Calendar identifier; the last non-empty path segment of the
    /// calendar collection URL.
    pub id: String,

    /// Human-readable display name (DAV:displayname).
    pub display_name: Option<String>,

    /// Free-form description (RFC 4791 §6.2.1).
    pub description: Option<String>,

    /// Display color, expressed as a CSS hex string (RFC 7986 §5.9).
    pub color: Option<String>,

    /// Collection change tag (RFC 6578 / CalendarServer ctag
    /// extension); incremented on every change to the calendar.
    pub ctag: Option<String>,

    /// Default time zone, expressed as a VTIMEZONE block (RFC 4791
    /// §5.2.2).
    pub tz: Option<String>,
}
