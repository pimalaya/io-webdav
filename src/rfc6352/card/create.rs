//! # Create card
//!
//! `create-card` coroutine: PUT raw vCard bytes against `<addressbook>/<id>`.
//!
//! The `id` is the resource name, used verbatim apart from the bytes a path
//! segment cannot hold raw, which are percent-encoded. io-webdav never appends
//! a file extension, so the caller owns the whole name.
//!
//! The returned [`CarddavCardCreateOk::id`] is that name, or the server's own
//! when it relocates the resource and reports it in a `Location` header.
//! Either way it is what read, update and delete address.
//!
//! Uses `If-None-Match: *` so the server rejects the PUT when a resource with
//! the same id already exists.
//!
//! A collection that already holds the card's `UID` refuses the PUT with the
//! `CARDDAV:no-uid-conflict` precondition (RFC 6352 §6.3.2), surfaced as
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
//!     rfc6352::card::create::CarddavCardCreate,
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
//! let mut coroutine = CarddavCardCreate::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/addressbooks/contacts/",
//!     "alice",
//!     vcard,
//! );
//! let mut arg = None;
//!
//! let created = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(created)) => break created,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("created {} (etag {:?})", created.id, created.etag);
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

/// Coroutine that creates a card.
#[derive(Debug)]
pub struct CarddavCardCreate {
    href: String,
    state: State,
}

impl CarddavCardCreate {
    /// Builds a new `create-card` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        id: &str,
        vcard: Vec<u8>,
    ) -> Self {
        let path = join_path(addressbook_path, id);
        let put = WebdavPut::new(WebdavPutArgs {
            base_url,
            auth,
            user_agent,
            path: &path,
            content_type: "text/vcard; charset=utf-8",
            body: vcard,
            if_match: None,
            if_none_match: Some("*"),
        });
        Self {
            href: sent_href(base_url, &path),
            state: State::WebdavPut(put),
        }
    }
}

impl WebdavCoroutine for CarddavCardCreate {
    type Yield = WebdavYield;
    type Return = Result<CarddavCardCreateOk, WebdavSendError>;

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
                let href = match response.header("location") {
                    Some(location) if id_from_location(location).is_some() => location.into(),
                    _ => mem::take(&mut self.href),
                };
                let id = id_from_location(&href).unwrap_or_default();
                WebdavCoroutineState::Complete(Ok(CarddavCardCreateOk { id, href, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavPut(WebdavPut),
}

/// Outcome of a successful [`CarddavCardCreate`] resume.
#[derive(Clone, Debug)]
pub struct CarddavCardCreateOk {
    /// Card resource id: the `Location` header's last path segment when the
    /// server returns one (its own name for the resource), otherwise the last
    /// segment of the request URL; either read as a listing reads it.
    pub id: String,
    /// The `Location` header as served when it names the resource, else the
    /// request URL's path: the key for a store keeping the server's spelling.
    pub href: String,
    /// Entity tag returned by the server, when present.
    pub etag: Option<String>,
}
