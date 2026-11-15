//! # Update card
//!
//! `update-card` coroutine: PUT raw vCard bytes against an existing card.
//!
//! Supports the optional `If-Match` precondition so callers can gate the write
//! on the last-known ETag (RFC 9110 §13.1.1).
//!
//! Not every server enforces it. Google CardDAV answers 204 and applies the
//! write whatever the `If-Match` value, a garbage one included (verified
//! 2026-10-01). Against such a server the precondition guards nothing: a
//! caller that needs it there compares the current ETag itself first, at the
//! cost of one more request and of a race between the check and the write.
//!
//! A collection that already holds the card's `UID` under another resource
//! refuses the PUT with the `CARDDAV:no-uid-conflict` precondition (RFC 6352
//! §6.3.2), surfaced as
//! [`DuplicateUid`](crate::rfc4918::send::WebdavSendError::DuplicateUid).
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
//!     rfc4918::WebdavAuth,
//!     rfc6352::card::update::CarddavCardUpdate,
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let vcard = b"BEGIN:VCARD\r\n...\r\nEND:VCARD\r\n".to_vec();
//! let mut coroutine = CarddavCardUpdate::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/addressbooks/contacts/",
//!     "alice",
//!     vcard,
//!     Some("\"abc123\""),
//! );
//! let mut arg = None;
//!
//! let updated = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(updated)) => break updated,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("updated {} (etag {:?})", updated.id, updated.etag);
//! ```

use core::mem;

use alloc::{string::String, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth, id_from_location,
        put::{WebdavPut, WebdavPutArgs, duplicate_uid},
        read_etag,
        send::{WebdavSendError, WebdavSendOk},
        sent_href,
    },
    rfc6352::card::join_path,
};

/// Coroutine that updates a card.
#[derive(Debug)]
pub struct CarddavCardUpdate {
    href: String,
    state: State,
}

impl CarddavCardUpdate {
    /// Builds a new `update-card` coroutine. `id` is the resource id, as a
    /// listing returns it (`CarddavCardEntry::id`).
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        id: &str,
        vcard: Vec<u8>,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(addressbook_path, id);
        let put = WebdavPut::new(WebdavPutArgs {
            base_url,
            auth,
            user_agent,
            path: &path,
            content_type: "text/vcard; charset=utf-8",
            body: vcard,
            if_match,
            if_none_match: None,
        });
        Self {
            href: sent_href(base_url, &path),
            state: State::WebdavPut(put),
        }
    }
}

impl WebdavCoroutine for CarddavCardUpdate {
    type Yield = WebdavYield;
    type Return = Result<CarddavCardUpdateOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavPut(put) => {
                let ok = match put.resume(arg) {
                    WebdavCoroutineState::Yielded(yielded) => {
                        return WebdavCoroutineState::Yielded(yielded);
                    }
                    WebdavCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(duplicate_uid(err)));
                    }
                    WebdavCoroutineState::Complete(Ok(ok)) => ok,
                };

                let WebdavSendOk { response, .. } = ok;
                let etag = read_etag(&response);
                let href = mem::take(&mut self.href);
                let id = id_from_location(&href).unwrap_or_default();
                WebdavCoroutineState::Complete(Ok(CarddavCardUpdateOk { id, href, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavPut(WebdavPut),
}

/// Outcome of a successful [`CarddavCardUpdate`] resume.
#[derive(Clone, Debug)]
pub struct CarddavCardUpdateOk {
    /// Card resource id: the last segment of the request URL, read as a
    /// listing reads it.
    pub id: String,
    /// The request URL's path as sent, percent-encoding kept: the key for a
    /// store keeping the server's spelling, which [`id`](Self::id) may decode.
    pub href: String,
    /// Updated entity tag returned by the server, when present.
    pub etag: Option<String>,
}
