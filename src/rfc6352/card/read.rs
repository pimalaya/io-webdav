//! `read-card` coroutine: GET a card by its resource name.
//!
//! Stays byte-oriented: returns raw vCard bytes plus the response's ETag,
//! leaving the parse to vcard upstream.
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
//!     rfc6352::card::read::CarddavCardRead,
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine =
//!     CarddavCardRead::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/contacts/", "alice");
//! let mut arg = None;
//!
//! let card = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(card)) => break card,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} bytes, etag {:?}", card.data.len(), card.etag);
//! ```

use alloc::{string::String, vec::Vec};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        get::WebdavGet,
        read_etag,
        send::{WebdavSendError, WebdavSendOk},
    },
    rfc6352::card::join_path,
    webdav_try,
};

/// Coroutine that reads a card.
#[derive(Debug)]
pub struct CarddavCardRead {
    state: State,
}

impl CarddavCardRead {
    /// Builds a new `read-card` coroutine. `id` is the resource id exactly as
    /// the server returned it (`CarddavCardEntry::id`), used verbatim.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        id: &str,
    ) -> Self {
        let path = join_path(addressbook_path, id);
        Self {
            state: State::WebdavGet(WebdavGet::new(base_url, auth, user_agent, &path)),
        }
    }
}

impl WebdavCoroutine for CarddavCardRead {
    type Yield = WebdavYield;
    type Return = Result<CarddavCardBody, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavGet(get) => {
                let WebdavSendOk { response, body, .. } = webdav_try!(get, arg);
                let etag = read_etag(&response);
                WebdavCoroutineState::Complete(Ok(CarddavCardBody { data: body, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    WebdavGet(WebdavGet),
}

/// Card body plus optional ETag returned by [`CarddavCardRead`].
#[derive(Clone, Debug)]
pub struct CarddavCardBody {
    /// Raw vCard bytes.
    pub data: Vec<u8>,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}
