//! `delete-addressbook` coroutine: `DELETE` against an addressbook
//! collection.
//!
//! Lifted from io-addressbook/src/carddav/coroutines/delete-addressbook.rs.

use alloc::{format, string::String, vec::Vec};

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    delete::Delete,
    send::SendResult,
};

/// Coroutine that deletes an addressbook collection.
#[derive(Debug)]
pub struct DeleteAddressbook(Delete);

impl DeleteAddressbook {
    /// Builds a new `delete-addressbook` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        addressbook_id: &str,
    ) -> Self {
        let path = join_path(home_set_path, addressbook_id);
        Self(Delete::new(base_url, auth, user_agent, &path, None))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}

fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}
