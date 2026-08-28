//! Generic `PUT` coroutine (RFC 4918 §9.7).
//!
//! Sends a `PUT` against `path` with the caller-supplied body bytes and content
//! type. Stays byte-oriented: callers parse iCal/vCard upstream.
//!
//! Supports the optional `If-Match` and `If-None-Match` preconditions
//! (RFC 9110 §13.1) so callers can gate the write on a known ETag.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_webdav::{
//!     coroutine::{WebdavCoroutine, WebdavCoroutineState, WebdavYield},
//!     rfc4918::{
//!         WebdavAuth,
//!         put::{WebdavPut, WebdavPutArgs},
//!     },
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = WebdavPut::new(WebdavPutArgs {
//!     base_url: &base_url,
//!     auth: &auth,
//!     user_agent: "io-webdav",
//!     path: "/dav/calendars/personal/event-1.ics",
//!     content_type: "text/calendar; charset=utf-8",
//!     body: b"BEGIN:VCALENDAR\r\n...\r\nEND:VCALENDAR\r\n".to_vec(),
//!     if_match: None,
//!     if_none_match: Some("*"),
//! });
//! let mut arg = None;
//!
//! let ok = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(ok)) => break ok,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("keep-alive: {}", ok.keep_alive);
//! ```

use alloc::vec::Vec;

use log::{debug, trace};
use quick_xml::{Reader, events::Event};
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        request::WebdavRequest,
        send::{WebdavSendError, WebdavSendOk, WebdavSendRaw},
    },
};

/// Build inputs for a [`WebdavPut`] coroutine.
///
/// Uses a struct rather than positional arguments so callers can build the
/// request literal-style and skip the two optional precondition fields without
/// juggling positional `None`s.
#[derive(Clone, Debug)]
pub struct WebdavPutArgs<'a> {
    /// Base URL the request path is resolved against.
    pub base_url: &'a Url,
    /// Authentication scheme for the `Authorization` header.
    pub auth: &'a WebdavAuth,
    /// Value emitted as the `User-Agent` header.
    pub user_agent: &'a str,
    /// Resource path to PUT to, relative to `base_url`.
    pub path: &'a str,
    /// MIME type emitted as the `Content-Type` header.
    pub content_type: &'a str,
    /// Raw request body bytes.
    pub body: Vec<u8>,
    /// Optional `If-Match` ETag (RFC 9110 §13.1.1).
    pub if_match: Option<&'a str>,
    /// Optional `If-None-Match` ETag (RFC 9110 §13.1.2).
    pub if_none_match: Option<&'a str>,
}

/// Coroutine that runs a `PUT`.
#[derive(Debug)]
pub struct WebdavPut {
    state: State,
}

impl WebdavPut {
    /// Builds a new `PUT` coroutine.
    pub fn new(args: WebdavPutArgs<'_>) -> Self {
        let mut builder = WebdavRequest::put(args.base_url, args.auth, args.user_agent, args.path)
            .content_type(args.content_type);

        if let Some(etag) = args.if_match {
            builder = builder.if_match(etag);
        }

        if let Some(etag) = args.if_none_match {
            builder = builder.if_none_match(etag);
        }

        let request = builder.body(args.body);
        Self {
            state: State::Send(WebdavSendRaw::new(request)),
        }
    }
}

impl WebdavCoroutine for WebdavPut {
    type Yield = WebdavYield;
    type Return = Result<WebdavSendOk<Vec<u8>>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => send.resume(arg),
        }
    }
}

/// Turns a send failure saying the collection already holds the written `UID`
/// into [`WebdavSendError::DuplicateUid`], and leaves every other one alone.
///
/// RFC 4791 §5.3.2 and RFC 6352 §6.3.2 both give the refusal a name: a server
/// that will not hold two resources under one `UID` answers with the
/// `no-uid-conflict` precondition, and only recommends the 409 it wraps it in.
/// The precondition is therefore what is matched, exactly as
/// [`WebdavReport`](crate::rfc4918::report::WebdavReport) matches its own, a
/// 409 carrying none of it being any of the other conflicts a write meets. It
/// is read as an element rather than searched for as a substring, so a body
/// merely quoting the words is not one, and the namespace prefix is ignored,
/// which is how both flavours of the element reach the one variant.
pub(crate) fn duplicate_uid(err: WebdavSendError) -> WebdavSendError {
    let WebdavSendError::HttpStatus { status, body } = err else {
        return err;
    };

    let mut reader = Reader::from_str(&body);
    let mut refused = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                if element.local_name().as_ref() == NO_UID_CONFLICT {
                    refused = true;
                    break;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    if refused {
        debug!("WebDAV collection already holds the written UID");
        return WebdavSendError::DuplicateUid { status, body };
    }

    WebdavSendError::HttpStatus { status, body }
}

/// Local name of the precondition a duplicate `UID` is refused with, spelled
/// `CALDAV:no-uid-conflict` (RFC 4791 §5.3.2) and `CARDDAV:no-uid-conflict`
/// (RFC 6352 §6.3.2).
const NO_UID_CONFLICT: &[u8] = b"no-uid-conflict";

#[derive(Debug)]
enum State {
    Send(WebdavSendRaw),
}
