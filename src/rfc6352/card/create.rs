//! `create-card` coroutine: PUT raw vCard bytes against
//! `<addressbook>/<id>.vcf`.
//!
//! Uses `If-None-Match: *` so the server rejects the PUT when a
//! resource with the same id already exists.
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
//!     rfc6352::card::create::CreateCard,
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
//! let mut coroutine = CreateCard::new(
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

use alloc::{
    format,
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

/// Coroutine that creates a card.
#[derive(Debug)]
pub struct CreateCard {
    id: String,
    state: State,
}

impl CreateCard {
    /// Builds a new `create-card` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        id: &str,
        vcard: Vec<u8>,
    ) -> Self {
        let path = join_path(addressbook_path, &format!("{id}.vcf"));
        let put = Put::new(PutArgs {
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
            id: id.to_string(),
            state: State::Put(put),
        }
    }
}

impl WebdavCoroutine for CreateCard {
    type Yield = WebdavYield;
    type Return = Result<CreateCardOk, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Put(put) => {
                let SendOk { response, .. } = webdav_try!(put, arg);
                let etag = read_etag(&response);
                let id = mem::take(&mut self.id);
                WebdavCoroutineState::Complete(Ok(CreateCardOk { id, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Put(Put),
}

/// Outcome of a successful
/// [`CreateCard`] resume.
#[derive(Clone, Debug)]
pub struct CreateCardOk {
    /// Card identifier (as supplied by the caller).
    pub id: String,
    /// Entity tag returned by the server, when present.
    pub etag: Option<String>,
}
