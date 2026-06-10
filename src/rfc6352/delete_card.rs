//! `delete-card` coroutine: `DELETE` a card by id.
//!
//! Supports the optional `If-Match` precondition so callers can gate
//! the deletion on the last-known ETag (RFC 9110 §13.1.1).
//!
//! Lifted from io-addressbook/src/carddav/coroutines/delete-card.rs.

use alloc::{format, string::String, vec::Vec};

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    delete::Delete,
    send::SendResult,
};

/// Coroutine that deletes a card.
#[derive(Debug)]
pub struct DeleteCard(Delete);

impl DeleteCard {
    /// Builds a new `delete-card` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        addressbook_path: &str,
        card_id: &str,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(addressbook_path, card_id);
        Self(Delete::new(base_url, auth, user_agent, &path, if_match))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}

fn join_path(addressbook: &str, id: &str) -> String {
    let addressbook = addressbook.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{addressbook}/{id}.vcf")
}
