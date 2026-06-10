//! `create-addressbook` coroutine: extended `MKCOL` (RFC 5689)
//! against the addressbook home-set URL.
//!
//! Lifted from io-addressbook/src/carddav/coroutines/create-addressbook.rs.

use alloc::{format, string::String, vec::Vec};

use url::Url;

use crate::{
    rfc4918::{
        auth::WebdavAuth,
        mkcol::Mkcol,
        send::{Empty, SendResult},
    },
    rfc6352::addressbook::Addressbook,
};

const BODY: &str = include_str!("./create_addressbook.xml");

/// Coroutine that creates an addressbook collection.
#[derive(Debug)]
pub struct CreateAddressbook(Mkcol);

impl CreateAddressbook {
    /// Builds a new `create-addressbook` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        addressbook: &Addressbook,
    ) -> Self {
        let path = join_path(home_set_path, &addressbook.id);
        let body = format_body(addressbook).into_bytes();
        Self(Mkcol::new(base_url, auth, user_agent, &path, body))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Empty> {
        self.0.resume(arg)
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
