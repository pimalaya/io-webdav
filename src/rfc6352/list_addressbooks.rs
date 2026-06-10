//! `list-addressbooks` coroutine: PROPFIND Depth:1 against the
//! addressbook home-set URL and collect every child collection whose
//! resourcetype is `<C:addressbook/>`.
//!
//! Lifted from io-addressbook/src/carddav/coroutines/list-addressbooks.rs.

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    rfc4918::{
        auth::WebdavAuth,
        request::WebdavRequest,
        response::Multistatus,
        send::{Send, SendOk, SendResult},
    },
    rfc6352::addressbook::Addressbook,
};

const BODY: &str = include_str!("./list_addressbooks.xml");

/// Coroutine that lists addressbooks under `home_set_path`.
#[derive(Debug)]
pub struct ListAddressbooks(Send<Multistatus<Prop>>);

impl ListAddressbooks {
    /// Builds a new `list-addressbooks` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, home_set_path: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, home_set_path)
            .content_type_xml()
            .depth(1)
            .body(BODY.as_bytes().to_vec());
        Self(Send::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<BTreeSet<Addressbook>> {
        match self.0.resume(arg) {
            SendResult::Ok(ok) => {
                let addressbooks = collect(&ok);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: addressbooks,
                })
            }
            SendResult::WantsRead => SendResult::WantsRead,
            SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            SendResult::Err(err) => SendResult::Err(err),
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
    value.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
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
