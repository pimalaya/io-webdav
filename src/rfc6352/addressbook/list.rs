//! # List addressbooks
//!
//! `list-addressbooks` coroutine: PROPFIND Depth:1 against the addressbook
//! home-set URL, collecting every child collection whose resourcetype is
//! `<C:addressbook/>`.
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
//!     rfc6352::addressbook::list::CarddavAddressbookList,
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
//!     CarddavAddressbookList::new(&base_url, &auth, "io-webdav", "/dav/addressbooks/");
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
        WebdavAuth, WebdavChildNames, WebdavReportSet, collection_id, non_empty,
        propfind::WebdavPropfind,
        send::WebdavSendError,
        typed::{WebdavMultistatusOf, WebdavResponseOf},
    },
    rfc6352::addressbook::{ADDRESSBOOK, CarddavAddressbook, LIST_PROPS},
    webdav_try,
};

/// Coroutine that lists addressbooks under `home_set_path`.
#[derive(Debug)]
pub struct CarddavAddressbookList {
    home_set_path: String,
    state: State,
}

impl CarddavAddressbookList {
    /// Builds a new `list-addressbooks` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, home_set_path: &str) -> Self {
        let propfind =
            WebdavPropfind::new(base_url, auth, user_agent, home_set_path, 1, LIST_PROPS);
        Self {
            home_set_path: home_set_path.to_string(),
            state: State::WebdavPropfind(propfind),
        }
    }
}

impl WebdavCoroutine for CarddavAddressbookList {
    type Yield = WebdavYield;
    type Return = Result<BTreeSet<CarddavAddressbook>, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavPropfind(propfind) => {
                let multistatus = webdav_try!(propfind, arg);
                let addressbooks = multistatus
                    .responses
                    .into_iter()
                    .filter_map(|response| from_response(response, &self.home_set_path))
                    .collect();
                WebdavCoroutineState::Complete(Ok(addressbooks))
            }
        }
    }
}

/// The properties of [`LIST_PROPS`], as a typed multistatus reads them.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct AddressbookProps {
    resourcetype: Option<WebdavChildNames>,
    displayname: Option<String>,
    addressbook_description: Option<String>,
    addressbook_color: Option<String>,
    getctag: Option<String>,
    sync_token: Option<String>,
    supported_report_set: Option<WebdavReportSet>,
}

fn from_response(
    response: WebdavResponseOf<AddressbookProps>,
    home_set_path: &str,
) -> Option<CarddavAddressbook> {
    let props = response.props;

    if !props
        .resourcetype
        .is_some_and(|types| types.0.contains_key(ADDRESSBOOK.local))
    {
        trace!("skip non-addressbook response {}", response.href);
        return None;
    }

    let id = collection_id(home_set_path, &response.href);
    if id.is_empty() {
        return None;
    }

    Some(CarddavAddressbook {
        id,
        display_name: non_empty(props.displayname),
        description: non_empty(props.addressbook_description),
        color: non_empty(props.addressbook_color),
        ctag: non_empty(props.getctag),
        sync_token: non_empty(props.sync_token),
        supported_reports: props
            .supported_report_set
            .map(WebdavReportSet::reports)
            .unwrap_or_default(),
    })
}

#[derive(Debug)]
enum State {
    WebdavPropfind(WebdavPropfind<WebdavMultistatusOf<AddressbookProps>>),
}
