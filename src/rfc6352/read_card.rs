//! `read-card` coroutine: GET a card by id.
//!
//! Stays byte-oriented: returns raw vCard bytes plus the response's
//! ETag so io-addressbook can run calcard upstream.
//!
//! Lifted from io-addressbook/src/carddav/coroutines/read-card.rs.

use alloc::{
    format,
    string::String,
    vec::Vec,
};

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    get::Get,
    put::read_etag,
    send::{SendOk, SendResult},
};

/// Card body plus optional ETag returned by [`ReadCard`].
#[derive(Clone, Debug)]
pub struct CardBody {
    /// Raw vCard bytes.
    pub data: Vec<u8>,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Coroutine that reads a card.
#[derive(Debug)]
pub struct ReadCard(Get);

impl ReadCard {
    /// Builds a new `read-card` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        card_id: &str,
    ) -> Self {
        let path = join_path(addressbook_path, card_id);
        Self(Get::new(base_url, auth, user_agent, &path))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<CardBody> {
        match self.0.resume(arg) {
            SendResult::Ok(ok) => {
                let etag = read_etag(&ok.response);
                let body = CardBody {
                    data: ok.body,
                    etag,
                };
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body,
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
