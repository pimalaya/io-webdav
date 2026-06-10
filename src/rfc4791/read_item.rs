//! `read-item` coroutine: GET a calendar item by id.
//!
//! Stays byte-oriented: returns raw iCalendar bytes plus the
//! response's `ETag` so io-calendar can run calcard upstream.
//!
//! Lifted from io-calendar/src/caldav/coroutines/read-item.rs.

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

/// Item body plus optional ETag returned by [`ReadItem`].
#[derive(Clone, Debug)]
pub struct ItemBody {
    /// Raw iCalendar bytes.
    pub data: Vec<u8>,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Coroutine that reads a calendar item.
#[derive(Debug)]
pub struct ReadItem(Get);

impl ReadItem {
    /// Builds a new `read-item` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        item_id: &str,
    ) -> Self {
        let path = join_path(calendar_path, item_id);
        Self(Get::new(base_url, auth, user_agent, &path))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<ItemBody> {
        match self.0.resume(arg) {
            SendResult::Ok(ok) => {
                let etag = read_etag(&ok.response);
                let body = ItemBody {
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

fn join_path(calendar: &str, id: &str) -> String {
    let calendar = calendar.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{calendar}/{id}.ics")
}
