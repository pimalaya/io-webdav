//! Generic WebDAV `Send` coroutine.
//!
//! Wraps [`io_http::rfc9112::send::Http11Send`] and adds quick-xml
//! deserialization of the response body into the caller-chosen `T`.
//! Use [`SendRaw`] when the body should stay as raw bytes (e.g. `GET`
//! / `PUT` of an iCal/vCard resource).
//!
//! Like the JMAP send coroutine, all I/O is hoisted: the coroutine
//! emits [`WantsRead`] / [`WantsWrite`] and the caller does the
//! actual stream work.
//!
//! [`WantsRead`]: SendResult::WantsRead
//! [`WantsWrite`]: SendResult::WantsWrite

use core::marker::PhantomData;

use alloc::{string::String, vec::Vec};

use io_http::{
    rfc9110::{request::HttpRequest, response::HttpResponse},
    rfc9112::send::{Http11Send, Http11SendError, Http11SendResult},
};
use log::trace;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

/// Successful outcome of a WebDAV [`Send`] coroutine.
#[derive(Debug)]
pub struct SendOk<T> {
    pub response: HttpResponse,
    pub keep_alive: bool,
    pub body: T,
}

/// Errors that can occur during a WebDAV send.
#[derive(Debug, Error)]
pub enum SendError {
    #[error("WebDAV server returned HTTP {0}: {1}")]
    HttpStatus(u16, String),
    #[error("Parse WebDAV XML response body error: {0}")]
    ParseXmlResponseBody(#[source] quick_xml::DeError),
    #[error("WebDAV server returned unexpected redirect")]
    UnexpectedRedirect,

    #[error(transparent)]
    Send(#[from] Http11SendError),
}

/// Result returned by [`Send::resume`] / [`SendRaw::resume`].
#[derive(Debug)]
pub enum SendResult<T> {
    /// The coroutine has successfully terminated.
    Ok(SendOk<T>),
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The coroutine encountered an error.
    Err(SendError),
}

/// Coroutine that sends a WebDAV request and deserializes the response
/// body as XML into `T`.
#[derive(Debug)]
pub struct Send<T: for<'a> Deserialize<'a>> {
    phantom: PhantomData<T>,
    send: Http11Send,
}

impl<T: for<'a> Deserialize<'a>> Send<T> {
    /// Builds a new `Send` coroutine. `request` must already carry its
    /// body bytes (via [`crate::rfc4918::request::WebdavRequest::body`]).
    pub fn new(request: HttpRequest) -> Self {
        trace!("send WebDAV request to {}", request.url);

        Self {
            phantom: PhantomData,
            send: Http11Send::new(request),
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<T> {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => {
                let body = String::from_utf8_lossy(&response.body);
                trace!("WebDAV response body: {body}");

                if !response.status.is_success() {
                    let err = SendError::HttpStatus(*response.status, body.into_owned());
                    return SendResult::Err(err);
                }

                let parsed = match quick_xml::de::from_str::<T>(&body) {
                    Ok(parsed) => parsed,
                    Err(err) => return SendResult::Err(SendError::ParseXmlResponseBody(err)),
                };

                SendResult::Ok(SendOk {
                    response,
                    keep_alive,
                    body: parsed,
                })
            }
            Http11SendResult::WantsRead => SendResult::WantsRead,
            Http11SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            Http11SendResult::WantsRedirect { .. } => SendResult::Err(SendError::UnexpectedRedirect),
            Http11SendResult::Err(err) => SendResult::Err(err.into()),
        }
    }
}

/// Coroutine that sends a WebDAV request and returns the response body
/// as raw bytes (no XML parsing).
///
/// Used by `GET` / `PUT` / `DELETE` against an iCal/vCard resource:
/// io-webdav stays byte-oriented and lets callers run calcard.
#[derive(Debug)]
pub struct SendRaw {
    send: Http11Send,
}

impl SendRaw {
    /// Builds a new `SendRaw` coroutine. `request` must already carry
    /// its body bytes.
    pub fn new(request: HttpRequest) -> Self {
        trace!("send WebDAV request to {}", request.url);

        Self {
            send: Http11Send::new(request),
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        match self.send.resume(arg) {
            Http11SendResult::Ok {
                response,
                keep_alive,
                ..
            } => {
                if !response.status.is_success() {
                    let body = String::from_utf8_lossy(&response.body).into_owned();
                    let err = SendError::HttpStatus(*response.status, body);
                    return SendResult::Err(err);
                }

                let body = response.body.clone();
                SendResult::Ok(SendOk {
                    response,
                    keep_alive,
                    body,
                })
            }
            Http11SendResult::WantsRead => SendResult::WantsRead,
            Http11SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            Http11SendResult::WantsRedirect { .. } => SendResult::Err(SendError::UnexpectedRedirect),
            Http11SendResult::Err(err) => SendResult::Err(err.into()),
        }
    }
}

/// Marker for coroutines that ignore the response body. Implements
/// [`serde::Deserialize`] so it can stand in for `T` in [`Send<T>`].
#[derive(Debug)]
pub struct Empty;

impl<'de> Deserialize<'de> for Empty {
    fn deserialize<D: Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Ok(Empty)
    }
}
