//! `update-calendar` coroutine: `PROPPATCH` against a calendar
//! collection.
//!
//! Lifted from io-calendar/src/caldav/coroutines/update-calendar.rs.

use alloc::{format, string::String, vec::Vec};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    rfc4791::calendar::Calendar,
    rfc4918::{
        auth::WebdavAuth,
        proppatch::Proppatch,
        send::{SendOk, SendResult},
        response::MkcolResponse,
    },
};

const BODY: &str = include_str!("./update_calendar.xml");

/// Coroutine that updates a calendar collection's properties.
#[derive(Debug)]
pub struct UpdateCalendar(Proppatch<Prop>);

impl UpdateCalendar {
    /// Builds a new `update-calendar` coroutine targeting
    /// `home_set_path` joined with `calendar.id`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        home_set_path: &str,
        calendar: &Calendar,
    ) -> Self {
        let path = join_path(home_set_path, &calendar.id);
        let body = format_body(calendar).into_bytes();
        Self(Proppatch::new(base_url, auth, user_agent, &path, body))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<()> {
        match self.0.resume(arg) {
            SendResult::Ok(ok) => {
                log_propstats(&ok);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: (),
                })
            }
            SendResult::WantsRead => SendResult::WantsRead,
            SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            SendResult::Err(err) => SendResult::Err(err),
        }
    }
}

fn log_propstats(ok: &SendOk<MkcolResponse<Prop>>) {
    let Some(propstats) = &ok.body.propstats else {
        return;
    };

    for propstat in propstats {
        if !propstat.status.is_success() {
            trace!("skip propstat with non-2xx status");
            continue;
        }

        if let Some(name) = &propstat.prop.displayname {
            trace!("calendar displayname updated: {name}");
        }

        if let Some(desc) = &propstat.prop.calendar_description {
            trace!("calendar description updated: {desc}");
        }

        if let Some(color) = &propstat.prop.calendar_color {
            trace!("calendar color updated: {color}");
        }
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

    BODY.replacen("{}", &name, 1).replacen("{}", &color, 1).replacen("{}", &description, 1)
}

fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}

/// `<prop>` payload echoed by a `PROPPATCH` response.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub displayname: Option<String>,
    pub calendar_color: Option<String>,
    pub calendar_description: Option<String>,
}
