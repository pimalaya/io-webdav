//! `read-card` coroutine: GET a card by its resource name.
//!
//! Stays byte-oriented: returns raw vCard bytes plus the response's
//! ETag so io-addressbook can run calcard upstream.
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
//!     rfc6352::card::read::ReadCard,
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine =
//!     ReadCard::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/contacts/", "alice");
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

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth,
        get::Get,
        read_etag,
        send::{SendError, SendOk},
    },
    rfc6352::card::{types::CardBody, utils::join_path},
    webdav_try,
};

/// Coroutine that reads a card.
#[derive(Debug)]
pub struct ReadCard {
    state: State,
}

impl ReadCard {
    /// Builds a new `read-card` coroutine. `card_uri` is the resource
    /// name as the server returned it (`CardEntry::uri`).
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        card_uri: &str,
    ) -> Self {
        let path = join_path(addressbook_path, card_uri);
        Self {
            state: State::Get(Get::new(base_url, auth, user_agent, &path)),
        }
    }
}

impl WebdavCoroutine for ReadCard {
    type Yield = WebdavYield;
    type Return = Result<CardBody, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Get(get) => {
                let SendOk { response, body, .. } = webdav_try!(get, arg);
                let etag = read_etag(&response);
                WebdavCoroutineState::Complete(Ok(CardBody { data: body, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Get(Get),
}
