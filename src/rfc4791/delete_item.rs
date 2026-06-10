//! `delete-item` coroutine: `DELETE` a calendar item by id.
//!
//! Supports the optional `If-Match` precondition so callers can gate
//! the deletion on the last-known ETag (RFC 9110 §13.1.1).
//!
//! Lifted from io-calendar/src/caldav/coroutines/delete-item.rs.

use alloc::{format, string::String, vec::Vec};

use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    delete::Delete,
    send::SendResult,
};

/// Coroutine that deletes a calendar item.
#[derive(Debug)]
pub struct DeleteItem(Delete);

impl DeleteItem {
    /// Builds a new `delete-item` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        item_id: &str,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(calendar_path, item_id);
        Self(Delete::new(base_url, auth, user_agent, &path, if_match))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Vec<u8>> {
        self.0.resume(arg)
    }
}

fn join_path(calendar: &str, id: &str) -> String {
    let calendar = calendar.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{calendar}/{id}.ics")
}
