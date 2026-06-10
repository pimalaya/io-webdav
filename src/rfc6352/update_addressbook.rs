//! `update-addressbook` coroutine: `PROPPATCH` against an
//! addressbook collection.
//!
//! Lifted from io-addressbook/src/carddav/coroutines/update-addressbook.rs.

use alloc::{format, string::String, vec::Vec};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    rfc4918::{
        auth::WebdavAuth,
        proppatch::Proppatch,
        response::MkcolResponse,
        send::{SendOk, SendResult},
    },
    rfc6352::addressbook::Addressbook,
};

const BODY: &str = include_str!("./update_addressbook.xml");

/// Coroutine that updates an addressbook collection's properties.
#[derive(Debug)]
pub struct UpdateAddressbook(Proppatch<Prop>);

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
        let body = format_body(addressbook).into_bytes();
        Self(Proppatch::new(base_url, auth, user_agent, &path, body))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<()> {
        match self.0.resume(arg) {
            SendResult::Ok(ok) => {
                log_propstats(&ok);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: (),
                })
            }
            SendResult::WantsRead => SendResult::WantsRead,
            SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            SendResult::Err(err) => SendResult::Err(err),
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

fn format_body(addressbook: &Addressbook) -> String {
    let name = match &addressbook.display_name {
        Some(value) => format!("<displayname>{value}</displayname>"),
        None => String::new(),
    };

    let color = match &addressbook.color {
        Some(value) => format!("<I:addressbook-color>{value}</I:addressbook-color>"),
        None => String::new(),
    };

    let description = match &addressbook.description {
        Some(value) => format!("<C:addressbook-description>{value}</C:addressbook-description>"),
        None => String::new(),
    };

    BODY.replacen("{}", &name, 1).replacen("{}", &color, 1).replacen("{}", &description, 1)
}

fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}

/// `<prop>` payload echoed by a `PROPPATCH` response.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub displayname: Option<String>,
    pub addressbook_color: Option<String>,
    pub addressbook_description: Option<String>,
}
