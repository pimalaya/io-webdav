//! Shared CardDAV card types returned by the list/read/create/update
//! coroutines.

use alloc::{string::String, vec::Vec};

/// Card reference (id plus ETag, no body) returned by
/// [`EnumCards`](crate::rfc6352::card::enumerate::EnumCards).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CardRef {
    /// Display identifier: [`uri`](Self::uri) with any `.vcf` stripped.
    pub id: String,

    /// Resource name (last path segment of the href), exactly as the
    /// server returned it; the addressing key of the read, update and
    /// delete coroutines. Servers are not required to suffix `.vcf`.
    pub uri: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Raw card entry returned by
/// [`ListCards`](crate::rfc6352::card::list::ListCards).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CardEntry {
    /// Display identifier: [`uri`](Self::uri) with any `.vcf` stripped.
    pub id: String,

    /// Resource name (last path segment of the href), exactly as the
    /// server returned it; the addressing key of the read, update and
    /// delete coroutines. Servers are not required to suffix `.vcf`.
    pub uri: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,

    /// Raw vCard bytes (`address-data`).
    pub data: Vec<u8>,
}

/// Card body plus optional ETag returned by
/// [`ReadCard`](crate::rfc6352::card::read::ReadCard).
#[derive(Clone, Debug)]
pub struct CardBody {
    /// Raw vCard bytes.
    pub data: Vec<u8>,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Outcome of a successful
/// [`CreateCard`](crate::rfc6352::card::create::CreateCard) resume.
#[derive(Clone, Debug)]
pub struct CreateCardOk {
    /// Card identifier (as supplied by the caller).
    pub id: String,
    /// Entity tag returned by the server, when present.
    pub etag: Option<String>,
}

/// Outcome of a successful
/// [`UpdateCard`](crate::rfc6352::card::update::UpdateCard) resume.
#[derive(Clone, Debug)]
pub struct UpdateCardOk {
    /// Card resource name (as supplied by the caller).
    pub uri: String,
    /// Updated entity tag returned by the server, when present.
    pub etag: Option<String>,
}
