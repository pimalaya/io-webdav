//! `create-item` coroutine: PUT raw iCalendar bytes against
//! `<calendar>/<id>.ics`.
//!
//! Uses `If-None-Match: *` so the server rejects the PUT when a
//! resource with the same id already exists (RFC 4791 §5.3.2).
//!
//! Lifted from io-calendar/src/caldav/coroutines/create-item.rs.

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

/// Outcome of a successful [`CreateItem`] resume.
#[derive(Clone, Debug)]
pub struct CreateItemOk {
    /// Item identifier (as supplied by the caller).
    pub id: String,
    /// Entity tag returned by the server, when present.
    pub etag: Option<String>,
}

/// Coroutine that creates a calendar item.
#[derive(Debug)]
pub struct CreateItem {
    id: String,
    put: Put,
}

impl CreateItem {
    /// Builds a new `create-item` coroutine targeting
    /// `<calendar_path>/<id>.ics`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        id: &str,
        ical: Vec<u8>,
    ) -> Self {
        let path = join_path(calendar_path, id);
        let put = Put::new(
            base_url,
            auth,
            user_agent,
            &path,
            "text/calendar; charset=utf-8",
            ical,
            None,
            Some("*"),
        );
        Self {
            id: id.to_string(),
            put,
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<CreateItemOk> {
        match self.put.resume(arg) {
            SendResult::Ok(ok) => {
                let etag = read_etag(&ok.response);
                let id = core::mem::take(&mut self.id);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: CreateItemOk { id, etag },
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
