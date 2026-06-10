//! Shared `Addressbook` type returned by CardDAV list/create
//! coroutines.

use alloc::string::String;

use serde::{Deserialize, Serialize};

/// A CardDAV addressbook collection (RFC 6352 §5).
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub struct Addressbook {
    /// Addressbook identifier; the last non-empty path segment of the
    /// addressbook collection URL.
    pub id: String,

    /// Human-readable display name (DAV:displayname).
    pub display_name: Option<String>,

    /// Free-form description (RFC 6352 §6.2.1).
    pub description: Option<String>,

    /// Display color (custom inf-it.com extension, widely supported by
    /// CardDAV clients).
    pub color: Option<String>,
}
