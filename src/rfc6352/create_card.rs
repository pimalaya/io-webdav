//! `create-card` coroutine: PUT raw vCard bytes against
//! `<addressbook>/<id>.vcf`.
//!
//! Uses `If-None-Match: *` so the server rejects the PUT when a
//! resource with the same id already exists.
//!
//! Lifted from io-addressbook/src/carddav/coroutines/create-card.rs.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    put::{Put, read_etag},
    send::{SendOk, SendResult},
};

/// Outcome of a successful [`CreateCard`] resume.
#[derive(Clone, Debug)]
pub struct CreateCardOk {
    /// Card identifier (as supplied by the caller).
    pub id: String,
    /// Entity tag returned by the server, when present.
    pub etag: Option<String>,
}

/// Coroutine that creates a card.
#[derive(Debug)]
pub struct CreateCard {
    id: String,
    put: Put,
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
        let path = join_path(addressbook_path, id);
        let put = Put::new(
            base_url,
            auth,
            user_agent,
            &path,
            "text/vcard; charset=utf-8",
            vcard,
            None,
            Some("*"),
        );
        Self {
            id: id.to_string(),
            put,
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<CreateCardOk> {
        match self.put.resume(arg) {
            SendResult::Ok(ok) => {
                let etag = read_etag(&ok.response);
                let id = core::mem::take(&mut self.id);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: CreateCardOk { id, etag },
                })
            }
            SendResult::WantsRead => SendResult::WantsRead,
            SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            SendResult::Err(err) => SendResult::Err(err),
        }
    }
}

fn join_path(addressbook: &str, id: &str) -> String {
    let addressbook = addressbook.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{addressbook}/{id}.vcf")
}
