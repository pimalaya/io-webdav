//! `update-item` coroutine: PUT raw iCalendar bytes against an
//! existing calendar item.
//!
//! Supports the optional `If-Match` precondition so callers can gate
//! the write on the last-known ETag (RFC 9110 §13.1.1).
//!
//! Lifted from io-calendar/src/caldav/coroutines/update-item.rs (which
//! aliased create-item); this variant emits `If-Match` instead of
//! `If-None-Match`.

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

/// Outcome of a successful [`UpdateItem`] resume.
#[derive(Clone, Debug)]
pub struct UpdateItemOk {
    /// Item identifier (as supplied by the caller).
    pub id: String,
    /// Updated entity tag returned by the server, when present.
    pub etag: Option<String>,
}

/// Coroutine that updates a calendar item.
#[derive(Debug)]
pub struct UpdateItem {
    id: String,
    put: Put,
}

impl UpdateItem {
    /// Builds a new `update-item` coroutine.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        id: &str,
        ical: Vec<u8>,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(calendar_path, id);
        let put = Put::new(
            base_url,
            auth,
            user_agent,
            &path,
            "text/calendar; charset=utf-8",
            ical,
            if_match,
            None,
        );
        Self {
            id: id.to_string(),
            put,
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<UpdateItemOk> {
        match self.put.resume(arg) {
            SendResult::Ok(ok) => {
                let etag = read_etag(&ok.response);
                let id = core::mem::take(&mut self.id);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: UpdateItemOk { id, etag },
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
