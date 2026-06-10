//! WebDAV shared types: the authentication scheme and the XML response
//! shapes (RFC 4918 §14), generic over the `<prop>` payload so callers
//! can plug their own `serde::Deserialize` shape per coroutine.

use alloc::{string::String, vec::Vec};

use memchr::memmem;
use secrecy::SecretString;
use serde::Deserialize;
use url::{ParseError, Url};

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

/// Multistatus body returned by `PROPFIND` / `REPORT` (RFC 4918 §14.16).
#[derive(Clone, Debug, Deserialize)]
pub struct Multistatus<T> {
    #[serde(rename = "response")]
    pub responses: Option<Vec<PropstatResponse<T>>>,
}

/// Top-level body of an `MKCOL` extended response (RFC 5689 §3).
#[derive(Clone, Debug, Deserialize)]
pub struct MkcolResponse<T> {
    #[serde(rename = "propstat")]
    pub propstats: Option<Vec<Propstat<T>>>,
}

/// Single entry inside a [`Multistatus`].
#[derive(Clone, Debug, Deserialize)]
pub struct PropstatResponse<T> {
    pub href: Value,
    pub status: Option<Status>,
    #[serde(rename = "propstat")]
    pub propstats: Option<Vec<Propstat<T>>>,
}

/// Bare `<status>` entry returned by `DELETE` / `MOVE` / `COPY`.
#[derive(Clone, Debug, Deserialize)]
pub struct StatusResponse {
    pub status: Status,
}

/// Generic `propstat` triple: `<prop>` payload plus its HTTP status.
#[derive(Clone, Debug, Deserialize)]
pub struct Propstat<T> {
    pub prop: T,
    pub status: Status,
}

/// `<prop>` that only carries an `<href>` child (used by
/// current-user-principal, calendar-home-set, addressbook-home-set).
#[derive(Clone, Debug, Deserialize)]
pub struct HrefProp {
    pub href: Value,
}

impl HrefProp {
    /// Parses the inner `<href>` as a [`Url`], using `base_url` as the
    /// resolution base when the href is relative.
    pub fn url(&self, base_url: &Url) -> Result<Url, ParseError> {
        match Url::parse(&self.href.value) {
            Ok(url) => Ok(url),
            Err(ParseError::RelativeUrlWithoutBase) => base_url.join(&self.href.value),
            Err(err) => Err(err),
        }
    }
}

/// Wrapper for arbitrary text bodies inside `<href>`, `<status>` or any
/// `xs:string` element.
#[derive(Clone, Debug, Deserialize)]
pub struct Value {
    #[serde(rename = "$value", default)]
    pub value: String,
}

/// HTTP status line embedded in a multistatus response.
#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
pub struct Status(Value);

impl Status {
    /// Returns `true` when the status line carries a 2xx code.
    pub fn is_success(&self) -> bool {
        memmem::find(self.0.value.as_bytes(), b" 2").is_some()
    }

    /// Returns the raw status line text (e.g. `HTTP/1.1 200 OK`).
    pub fn as_str(&self) -> &str {
        &self.0.value
    }
}
