//! `update-card` coroutine: PUT raw vCard bytes against an existing card.
//!
//! Supports the optional `If-Match` precondition so callers can gate the write
//! on the last-known ETag (RFC 9110 §13.1.1).
//!
//! A collection that already holds the card's `UID` under another resource
//! refuses the PUT with the `CARDDAV:no-uid-conflict` precondition (RFC 6352
//! §6.3.2), surfaced as
//! [`DuplicateUid`](crate::rfc4918::send::WebdavSendError::DuplicateUid)
//! rather than as an opaque conflict.
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

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        put::{WebdavPut, WebdavPutArgs, duplicate_uid},
        read_etag,
        send::{WebdavSendError, WebdavSendOk},
    },
    rfc6352::card::join_path,
};

/// Coroutine that updates a card.
#[derive(Debug)]
pub struct CarddavCardUpdate {
    id: String,
    state: State,
}

impl CarddavCardUpdate {
    /// Builds a new `update-card` coroutine. `id` is the resource id exactly as
    /// the server returned it (`CarddavCardEntry::id`), used verbatim.
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
            id: id.to_string(),
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
                let id = mem::take(&mut self.id);
                WebdavCoroutineState::Complete(Ok(CarddavCardUpdateOk { id, etag }))
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
    /// Card resource id (the resource name supplied by the caller, used
    /// verbatim).
    pub id: String,
    /// Updated entity tag returned by the server, when present.
    pub etag: Option<String>,
}
