//! `update-card` coroutine: PUT raw vCard bytes against an existing
//! card.
//!
//! Supports the optional `If-Match` precondition so callers can gate
//! the write on the last-known ETag (RFC 9110 §13.1.1).
//!
//! Lifted from io-addressbook/src/carddav/coroutines/update-card.rs
//! (which aliased create-card); this variant emits `If-Match` instead
//! of `If-None-Match`.

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

/// Outcome of a successful [`UpdateCard`] resume.
#[derive(Clone, Debug)]
pub struct UpdateCardOk {
    /// Card identifier (as supplied by the caller).
    pub id: String,
    /// Updated entity tag returned by the server, when present.
    pub etag: Option<String>,
}

/// Coroutine that updates a card.
#[derive(Debug)]
pub struct UpdateCard {
    id: String,
    put: Put,
}

impl UpdateCard {
    /// Builds a new `update-card` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        id: &str,
        vcard: Vec<u8>,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(addressbook_path, id);
        let put = Put::new(
            base_url,
            auth,
            user_agent,
            &path,
            "text/vcard; charset=utf-8",
            vcard,
            if_match,
            None,
        );
        Self {
            id: id.to_string(),
            put,
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<UpdateCardOk> {
        match self.put.resume(arg) {
            SendResult::Ok(ok) => {
                let etag = read_etag(&ok.response);
                let id = core::mem::take(&mut self.id);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: UpdateCardOk { id, etag },
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
