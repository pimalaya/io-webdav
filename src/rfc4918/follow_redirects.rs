//! Send coroutine that surfaces 3xx redirects to the caller.
//!
//! Wraps [`crate::rfc4918::send`] and turns the underlying HTTP
//! `WantsRedirect` event into a typed [`FollowRedirectsResult::WantsRedirect`]
//! variant so the client can rebuild its connection and restart the
//! operation against the new target URL.
//!
//! Lifted from io-calendar/src/caldav/coroutines/follow-redirects.rs
//! and io-addressbook/src/carddav/coroutines/follow-redirects.rs.

use core::marker::PhantomData;

use alloc::{string::String, vec::Vec};

use io_http::{
    rfc9110::{request::HttpRequest, response::HttpResponse},
    rfc9112::send::{Http11Send, Http11SendError, Http11SendResult},
};
use log::trace;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::rfc4918::send::SendOk;

/// Errors that can occur during a redirect-aware WebDAV send.
#[derive(Debug, Error)]
pub enum FollowRedirectsError {
    #[error("WebDAV server returned HTTP {0}: {1}")]
    HttpStatus(u16, String),
    #[error("Parse WebDAV XML response body error: {0}")]
    ParseXmlResponseBody(#[source] quick_xml::DeError),

    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// Result returned by [`FollowRedirects::resume`].
#[derive(Debug)]
pub enum FollowRedirectsResult<T> {
    /// The coroutine has successfully terminated.
    Ok(SendOk<T>),
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The server responded with a 3xx redirect; the caller must
    /// reconnect to `url` (and reopen the connection when
    /// `!keep_alive || !same_origin`) and retry the operation.
    WantsRedirect {
        url: Url,
        keep_alive: bool,
        same_origin: bool,
    },
    /// The coroutine encountered an error.
    Err(FollowRedirectsError),
}

/// Coroutine that sends a WebDAV request, surfaces 3xx redirects via
/// [`FollowRedirectsResult::WantsRedirect`] and parses the success
/// body as XML into `T`.
#[derive(Debug)]
pub struct FollowRedirects<T: for<'a> Deserialize<'a>> {
    phantom: PhantomData<T>,
    send: Http11Send,
}

impl<T: for<'a> Deserialize<'a>> FollowRedirects<T> {
    /// Builds a new redirect-aware send coroutine. `request` must
    /// already carry its body bytes.
    pub fn new(request: HttpRequest) -> Self {
        trace!("send WebDAV request to {} (redirect-aware)", request.url);

        Self {
            phantom: PhantomData,
            send: Http11Send::new(request),
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> FollowRedirectsResult<T> {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => parse_ok(response, keep_alive),
            Http11SendResult::WantsRead => FollowRedirectsResult::WantsRead,
            Http11SendResult::WantsWrite(bytes) => FollowRedirectsResult::WantsWrite(bytes),
            Http11SendResult::WantsRedirect {
                url,
                keep_alive,
                same_origin,
                ..
            } => FollowRedirectsResult::WantsRedirect {
                url,
                keep_alive,
                same_origin,
            },
            Http11SendResult::Err(err) => FollowRedirectsResult::Err(err.into()),
        }
    }
}

fn parse_ok<T: for<'a> Deserialize<'a>>(
    response: HttpResponse,
    keep_alive: bool,
) -> FollowRedirectsResult<T> {
    let body = String::from_utf8_lossy(&response.body);
    trace!("WebDAV response body: {body}");

    if !response.status.is_success() {
        let err = FollowRedirectsError::HttpStatus(*response.status, body.into_owned());
        return FollowRedirectsResult::Err(err);
    }

    let parsed = match quick_xml::de::from_str::<T>(&body) {
        Ok(parsed) => parsed,
        Err(err) => return FollowRedirectsResult::Err(FollowRedirectsError::ParseXmlResponseBody(err)),
    };

    FollowRedirectsResult::Ok(SendOk {
        response,
        keep_alive,
        body: parsed,
    })
}
