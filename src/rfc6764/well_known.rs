//! `.well-known/caldav` and `.well-known/carddav` discovery (RFC 6764).
//!
//! Sends a single non-PROPFIND request against `/.well-known/caldav`
//! (or `/.well-known/carddav`) and surfaces the redirect target so the
//! caller can rebuild its [`crate::client::WebdavClientStd`] against
//! the actual server root.
//!
//! Lifted from io-calendar/src/caldav/coroutines/well-known.rs and
//! io-addressbook/src/carddav/coroutines/well-known.rs and unified
//! into a single coroutine parametrized by `WellKnownKind`.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use io_http::{
    rfc9110::{request::HttpRequest, response::HttpResponse},
    rfc9112::send::{Http11Send, Http11SendError, Http11SendResult},
};
use log::trace;
use thiserror::Error;
use url::Url;

use crate::rfc4918::auth::{WebdavAuth, emit_header};

/// Which RFC 6764 service to discover.
#[derive(Clone, Copy, Debug)]
pub enum WellKnownKind {
    /// CalDAV: `/.well-known/caldav` (RFC 6764 §5).
    Caldav,
    /// CardDAV: `/.well-known/carddav` (RFC 6764 §5).
    Carddav,
}

impl WellKnownKind {
    /// Returns the well-known path including the leading slash.
    pub fn path(self) -> &'static str {
        match self {
            Self::Caldav => "/.well-known/caldav",
            Self::Carddav => "/.well-known/carddav",
        }
    }
}

/// Errors that can occur during well-known discovery.
#[derive(Debug, Error)]
pub enum WellKnownError {
    #[error("Expected a 3xx redirection from .well-known, got HTTP {0}: {1}")]
    NotRedirected(u16, String),
    #[error("Missing Location header in .well-known response")]
    MissingLocationHeader,
    #[error("Invalid Location header URL `{0}`: {1}")]
    InvalidLocationUrl(String, #[source] url::ParseError),

    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// Result returned by [`WellKnown::resume`].
#[derive(Debug)]
pub enum WellKnownResult {
    /// The coroutine has successfully terminated; `url` is the
    /// redirect target.
    Ok { url: Url, keep_alive: bool },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(WellKnownError),
}

/// Well-known discovery coroutine.
#[derive(Debug)]
pub struct WellKnown {
    send: Http11Send,
}

impl WellKnown {
    /// Builds a new well-known discovery coroutine against
    /// `base_url`'s authority.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        kind: WellKnownKind,
    ) -> Self {
        let mut url = base_url.clone();
        url.set_path(kind.path());

        trace!("discover {kind:?} via {url}");

        let host = match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            (None, _) => String::new(),
        };

        let mut request = HttpRequest::get(url).header("User-Agent", user_agent);
        if !host.is_empty() {
            request = request.header("Host", host);
        }
        if let Some(value) = emit_header(auth) {
            request = request.header("Authorization", value);
        }

        Self {
            send: Http11Send::new(request),
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> WellKnownResult {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => match read_location(&response) {
                Ok(url) => WellKnownResult::Ok { url, keep_alive },
                Err(err) => WellKnownResult::Err(err),
            },
            Http11SendResult::WantsRead => WellKnownResult::WantsRead,
            Http11SendResult::WantsWrite(bytes) => WellKnownResult::WantsWrite(bytes),
            Http11SendResult::WantsRedirect {
                url, keep_alive, ..
            } => WellKnownResult::Ok { url, keep_alive },
            Http11SendResult::Err(err) => WellKnownResult::Err(err.into()),
        }
    }
}

fn read_location(response: &HttpResponse) -> Result<Url, WellKnownError> {
    let code = *response.status;
    if !(300..400).contains(&code) {
        let body = String::from_utf8_lossy(&response.body).into_owned();
        return Err(WellKnownError::NotRedirected(code, body));
    }

    let location = response
        .header("location")
        .ok_or(WellKnownError::MissingLocationHeader)?;

    Url::parse(location)
        .map_err(|err| WellKnownError::InvalidLocationUrl(location.into(), err))
}
