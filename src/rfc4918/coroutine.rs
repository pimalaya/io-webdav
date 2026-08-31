//! # Redirect yield
//!
//! Yield emitted by the redirect-capable WebDAV coroutines
//! (current-user-principal, calendar-home-set, addressbook-home-set).
//!
//! Their exchange may surface a 3xx, which the caller then chooses to follow or
//! to treat as an error.

use alloc::vec::Vec;

use io_http::rfc9110::send::HttpSendYield;
use url::Url;

/// Per-step yield of the redirect-capable WebDAV coroutines: the standard I/O
/// variants plus [`Self::WantsRedirect`].
#[derive(Debug)]
pub enum WebdavRedirectYield {
    /// Driver should read more bytes and feed them back on the next resume.
    WantsRead,
    /// Driver should write these bytes; the next resume typically takes `None`.
    WantsWrite(Vec<u8>),
    /// Server responded with a 3xx. The caller opens a new connection when
    /// `!keep_alive || !same_origin` and builds a fresh coroutine for `url`.
    WantsRedirect {
        /// Resolved redirect target, from the `Location` header.
        url: Url,
        /// Whether the server will keep the connection open.
        keep_alive: bool,
        /// Whether the redirect stays on the same scheme, host and port.
        same_origin: bool,
    },
}

impl From<HttpSendYield> for WebdavRedirectYield {
    fn from(y: HttpSendYield) -> Self {
        match y {
            HttpSendYield::WantsRead => Self::WantsRead,
            HttpSendYield::WantsWrite(bytes) => Self::WantsWrite(bytes),
            HttpSendYield::WantsRedirect {
                url,
                keep_alive,
                same_origin,
                ..
            } => Self::WantsRedirect {
                url,
                keep_alive,
                same_origin,
            },
        }
    }
}
