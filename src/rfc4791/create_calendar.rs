//! `create-calendar` coroutine: extended `MKCOL` (RFC 5689) against
//! the calendar home-set URL.
//!
//! Lifted from io-calendar/src/caldav/coroutines/create-calendar.rs.

use alloc::{format, string::String, vec::Vec};

use url::Url;

use crate::{
    rfc4791::calendar::Calendar,
    rfc4918::{
        auth::WebdavAuth,
        mkcol::Mkcol,
        send::{Empty, SendResult},
    },
};

const BODY: &str = include_str!("./create_calendar.xml");

/// Coroutine that creates a calendar collection.
#[derive(Debug)]
pub struct CreateCalendar(Mkcol);

impl CreateCalendar {
    /// Builds a new `create-calendar` coroutine targeting `home_set`
    /// joined with `calendar.id`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        calendar: &Calendar,
    ) -> Self {
        let path = join_path(home_set_path, &calendar.id);
        let body = format_body(calendar).into_bytes();
        Self(Mkcol::new(base_url, auth, user_agent, &path, body))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<Empty> {
        self.0.resume(arg)
    }
}

fn format_body(calendar: &Calendar) -> String {
    let name = match &calendar.display_name {
        Some(value) => format!("<displayname>{value}</displayname>"),
        None => String::new(),
    };

    let color = match &calendar.color {
        Some(value) => format!("<I:calendar-color>{value}</I:calendar-color>"),
        None => String::new(),
    };

    let description = match &calendar.description {
        Some(value) => format!("<C:calendar-description>{value}</C:calendar-description>"),
        None => String::new(),
    };

    format!("{}", BODY).replacen("{}", &name, 1).replacen("{}", &color, 1).replacen("{}", &description, 1)
}

fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}
