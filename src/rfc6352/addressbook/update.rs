//! `update-addressbook` coroutine: `PROPPATCH` against an
//! addressbook collection.
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
//!     rfc6352::addressbook::{Addressbook, update::UpdateAddressbook},
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let addressbook = Addressbook {
//!     id: "contacts".into(),
//!     display_name: Some("My Contacts".into()),
//!     ..Default::default()
//! };
//! let mut coroutine =
//!     UpdateAddressbook::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/", &addressbook);
//! let mut arg = None;
//!
//! loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(())) => break,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! }
//! ```

use core::fmt;

use alloc::string::String;

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        proppatch::Proppatch,
        send::{SendError, SendOk},
        {MkcolResponse, WebdavAuth},
    },
    rfc6352::addressbook::{
        types::Addressbook,
        utils::{format_body, join_path},
    },
    webdav_try,
};

const BODY: &str = include_str!("./update.xml");

/// Coroutine that updates an addressbook collection's properties.
#[derive(Debug)]
pub struct UpdateAddressbook {
    state: State,
}

impl UpdateAddressbook {
    /// Builds a new `update-addressbook` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        addressbook: &Addressbook,
    ) -> Self {
        let path = join_path(home_set_path, &addressbook.id);
        let body = format_body(BODY, addressbook).into_bytes();
        Self {
            state: State::Proppatch(Proppatch::new(base_url, auth, user_agent, &path, body)),
        }
    }
}

impl WebdavCoroutine for UpdateAddressbook {
    type Yield = WebdavYield;
    type Return = Result<(), SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("update-addressbook: {}", self.state);
        match &mut self.state {
            State::Proppatch(proppatch) => {
                let ok = webdav_try!(proppatch, arg);
                log_propstats(&ok);
                WebdavCoroutineState::Complete(Ok(()))
            }
        }
    }
}

fn log_propstats(ok: &SendOk<MkcolResponse<Prop>>) {
    let Some(propstats) = &ok.body.propstats else {
        return;
    };

    for propstat in propstats {
        if !propstat.status.is_success() {
            trace!("skip propstat with non-2xx status");
            continue;
        }

        if let Some(name) = &propstat.prop.displayname {
            trace!("addressbook displayname updated: {name}");
        }

        if let Some(desc) = &propstat.prop.addressbook_description {
            trace!("addressbook description updated: {desc}");
        }

        if let Some(color) = &propstat.prop.addressbook_color {
            trace!("addressbook color updated: {color}");
        }
    }
}

#[derive(Debug)]
enum State {
    Proppatch(Proppatch<Prop>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Proppatch(_) => f.write_str("proppatch"),
        }
    }
}

/// `<prop>` payload echoed by a `PROPPATCH` response.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub displayname: Option<String>,
    pub addressbook_color: Option<String>,
    pub addressbook_description: Option<String>,
}
