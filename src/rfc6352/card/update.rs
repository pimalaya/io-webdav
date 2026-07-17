//! `update-card` coroutine: PUT raw vCard bytes against an existing
//! card.
//!
//! Supports the optional `If-Match` precondition so callers can gate
//! the write on the last-known ETag (RFC 9110 §13.1.1).
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
//!     rfc6352::card::update::UpdateCard,
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let vcard = b"BEGIN:VCARD\r\n...\r\nEND:VCARD\r\n".to_vec();
//! let mut coroutine = UpdateCard::new(
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
        put::{Put, PutArgs},
        read_etag,
        send::{SendError, SendOk},
    },
    rfc6352::card::join_path,
    webdav_try,
};

/// Coroutine that updates a card.
#[derive(Debug)]
pub struct UpdateCard {
    id: String,
    state: State,
}

impl UpdateCard {
    /// Builds a new `update-card` coroutine. `id` is the resource id
    /// exactly as the server returned it (`CardEntry::id`), used
    /// verbatim.
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
        let put = Put::new(PutArgs {
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
            state: State::Put(put),
        }
    }
}

impl WebdavCoroutine for UpdateCard {
    type Yield = WebdavYield;
    type Return = Result<UpdateCardOk, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Put(put) => {
                let SendOk { response, .. } = webdav_try!(put, arg);
                let etag = read_etag(&response);
                let id = mem::take(&mut self.id);
                WebdavCoroutineState::Complete(Ok(UpdateCardOk { id, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Put(Put),
}

/// Outcome of a successful
/// [`UpdateCard`] resume.
#[derive(Clone, Debug)]
pub struct UpdateCardOk {
    /// Card resource id (the resource name supplied by the caller, used
    /// verbatim).
    pub id: String,
    /// Updated entity tag returned by the server, when present.
    pub etag: Option<String>,
}
