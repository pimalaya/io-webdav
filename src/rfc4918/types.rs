//! WebDAV shared types: the authentication scheme, the property
//! identifier, and the generic parsed multistatus body (RFC 4918 §14).
//!
//! Responses are parsed into a vocabulary-agnostic
//! [`Multistatus`]/[`ResponseEntry`]/[`PropItem`] bag rather than a
//! per-RFC `serde` shape: each RFC layer reads the property names it
//! knows and ignores the rest. See
//! [`parse_multistatus`](crate::rfc4918::parse_multistatus).

use alloc::{
    string::String,
    vec::{self, Vec},
};

use secrecy::SecretString;

/// Authentication scheme used by the WebDAV client.
///
/// Covers the three modes the CalDAV/CardDAV deployments handle in
/// practice: no auth, HTTP Basic (RFC 7617) and HTTP Bearer (RFC 6750).
/// Higher-level coroutines never observe the credential directly; they
/// only see the pre-formatted header value from
/// [`emit_header`](crate::rfc4918::emit_header).
#[derive(Clone, Debug, Default)]
pub enum WebdavAuth {
    /// No authentication; no `Authorization` header is emitted.
    #[default]
    None,

    /// HTTP Basic authentication (RFC 7617).
    Basic {
        username: String,
        password: SecretString,
    },

    /// HTTP Bearer authentication (RFC 6750).
    Bearer { token: SecretString },
}

/// An XML namespace: its URI plus the preferred prefix used when
/// serializing request bodies (the empty prefix means the default
/// namespace).
///
/// Each RFC layer owns the namespaces it speaks (`DAV:` in
/// [`crate::rfc4918`], CalDAV ones in [`crate::rfc4791`], CardDAV ones
/// in [`crate::rfc6352`]); the generic body generators only read these
/// fields, so they never need to know which namespaces exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Namespace {
    /// Namespace URI (e.g. `DAV:`).
    pub uri: &'static str,
    /// Preferred XML prefix (`""` for the default namespace).
    pub prefix: &'static str,
}

/// A WebDAV property identifier: an XML [`Namespace`] plus a local name
/// (RFC 4918 §15).
///
/// Each RFC layer owns its own vocabulary as `const` values (generic
/// DAV properties in [`crate::rfc4918`], calendar ones in
/// [`crate::rfc4791`], card ones in [`crate::rfc6352`]); there is no
/// central enum. Construct an ad-hoc value for any property the
/// constants do not cover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Property {
    /// XML namespace.
    pub ns: Namespace,
    /// Local element name (e.g. `displayname`).
    pub local: &'static str,
}

/// Parsed `multistatus` body returned by `PROPFIND` / `REPORT`
/// (RFC 4918 §14.16).
#[derive(Clone, Debug, Default)]
pub struct Multistatus {
    pub responses: Vec<ResponseEntry>,
}

impl IntoIterator for Multistatus {
    type Item = ResponseEntry;
    type IntoIter = vec::IntoIter<ResponseEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.responses.into_iter()
    }
}

/// A single `<response>` inside a [`Multistatus`]: its `href` plus the
/// properties returned under 2xx `propstat`s.
#[derive(Clone, Debug, Default)]
pub struct ResponseEntry {
    /// The `<href>` text, as returned by the server.
    pub href: String,
    /// Properties gathered from every 2xx `<propstat>` of this response.
    pub props: Vec<PropItem>,
}

impl ResponseEntry {
    /// Returns the property matching `prop` (by local name), if present.
    pub fn prop(&self, prop: Property) -> Option<&PropItem> {
        self.props.iter().find(|item| item.local == prop.local)
    }

    /// Returns `prop`'s trimmed text content when present and non-empty.
    pub fn text(&self, prop: Property) -> Option<&str> {
        self.prop(prop)
            .map(|item| item.text.trim())
            .filter(|text| !text.is_empty())
    }

    /// Returns `true` when `<resourcetype>` lists `ty` as a child
    /// (e.g. `<C:calendar/>`).
    pub fn has_resource_type(&self, resourcetype: Property, ty: Property) -> bool {
        self.prop(resourcetype)
            .is_some_and(|item| item.children.iter().any(|child| child == ty.local))
    }

    /// Returns the last non-empty path segment of [`href`](Self::href),
    /// the conventional collection / resource identifier.
    pub fn id(&self) -> &str {
        self.href
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
    }
}

/// A single property returned inside a `<prop>` element.
#[derive(Clone, Debug, Default)]
pub struct PropItem {
    /// Property local name (e.g. `displayname`, `resourcetype`).
    pub local: String,
    /// Concatenated descendant text (covers text properties and the
    /// `<href>` payload of principal / home-set properties).
    pub text: String,
    /// Local names of the direct child elements (e.g. `calendar`,
    /// `collection` under `<resourcetype>`).
    pub children: Vec<String>,
}
