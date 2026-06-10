//! `list-addressbooks` coroutine: PROPFIND Depth:1 against the
//! addressbook home-set URL, collecting every child collection whose
//! resourcetype is `<C:addressbook/>`.
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
//!     rfc6352::addressbook::list::ListAddressbooks,
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
//!     ListAddressbooks::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/");
//! let mut arg = None;
//!
//! let addressbooks = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(addressbooks)) => break addressbooks,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} addressbooks", addressbooks.len());
//! ```

use core::fmt;

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        request::WebdavRequest,
        send::{Send, SendError, SendOk},
        {Multistatus, WebdavAuth},
    },
    rfc6352::addressbook::types::Addressbook,
    webdav_try,
};

const BODY: &str = include_str!("./list.xml");

/// Coroutine that lists addressbooks under `home_set_path`.
#[derive(Debug)]
pub struct ListAddressbooks {
    state: State,
}

impl ListAddressbooks {
    /// Builds a new `list-addressbooks` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, home_set_path: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, home_set_path)
            .content_type_xml()
            .depth(1)
            .body(BODY.as_bytes().to_vec());
        Self {
            state: State::Send(Send::new(request)),
        }
    }
}

impl WebdavCoroutine for ListAddressbooks {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<Addressbook>, SendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("list-addressbooks: {}", self.state);
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                WebdavCoroutineState::Complete(Ok(collect(&ok)))
            }
        }
    }
}

fn collect(ok: &SendOk<Multistatus<Prop>>) -> BTreeSet<Addressbook> {
    let mut addressbooks = BTreeSet::new();

    let Some(responses) = &ok.body.responses else {
        return addressbooks;
    };

    for response in responses {
        trace!("process multistatus response");

        if let Some(status) = &response.status {
            if !status.is_success() {
                trace!("skip multistatus response with non-2xx status");
                continue;
            }
        }

        let Some(propstats) = &response.propstats else {
            continue;
        };

        let id = response
            .href
            .value
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();

        let mut addressbook = Addressbook {
            id,
            ..Default::default()
        };
        let mut is_addressbook = false;

        for propstat in propstats {
            if !propstat.status.is_success() {
                trace!("skip propstat with non-2xx status");
                continue;
            }

            if let Some(rtype) = &propstat.prop.resourcetype {
                if rtype.addressbook.is_some() {
                    is_addressbook = true;
                }
            }

            if let Some(name) = non_empty(propstat.prop.displayname.as_deref()) {
                addressbook.display_name = Some(name);
            }

            if let Some(desc) = non_empty(propstat.prop.addressbook_description.as_deref()) {
                addressbook.description = Some(desc);
            }

            if let Some(color) = non_empty(propstat.prop.addressbook_color.as_deref()) {
                addressbook.color = Some(color);
            }
        }

        if is_addressbook && !addressbook.id.is_empty() {
            addressbooks.insert(addressbook);
        }
    }

    addressbooks
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

#[derive(Debug)]
enum State {
    Send(Send<Multistatus<Prop>>),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(_) => f.write_str("send"),
        }
    }
}

/// `<prop>` payload returned by the list-addressbooks PROPFIND.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub resourcetype: Option<ResourceType>,
    pub displayname: Option<String>,
    pub addressbook_color: Option<String>,
    pub addressbook_description: Option<String>,
}

/// `<resourcetype>` element returned by the list-addressbooks PROPFIND.
#[derive(Clone, Debug, Deserialize)]
pub struct ResourceType {
    pub addressbook: Option<()>,
}
