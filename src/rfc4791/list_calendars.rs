//! `list-calendars` coroutine: PROPFIND Depth:1 against the calendar
//! home-set URL and collect every child collection whose resourcetype
//! is `<C:calendar/>`.
//!
//! Lifted from io-calendar/src/caldav/coroutines/list-calendars.rs and
//! extended with ctag and timezone properties.

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::{
    rfc4791::calendar::Calendar,
    rfc4918::{
        auth::WebdavAuth,
        request::WebdavRequest,
        response::Multistatus,
        send::{Send, SendOk, SendResult},
    },
};

const BODY: &str = include_str!("./list_calendars.xml");

/// Coroutine that lists calendars under `home_set_path`.
#[derive(Debug)]
pub struct ListCalendars(Send<Multistatus<Prop>>);

impl ListCalendars {
    /// Builds a new `list-calendars` coroutine.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str, home_set_path: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, home_set_path)
            .depth(1)
            .content_type_xml()
            .body(BODY.as_bytes().to_vec());
        Self(Send::new(request))
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> SendResult<BTreeSet<Calendar>> {
        match self.0.resume(arg) {
            SendResult::Ok(ok) => {
                let calendars = collect(&ok);
                SendResult::Ok(SendOk {
                    response: ok.response,
                    keep_alive: ok.keep_alive,
                    body: calendars,
                })
            }
            SendResult::WantsRead => SendResult::WantsRead,
            SendResult::WantsWrite(bytes) => SendResult::WantsWrite(bytes),
            SendResult::Err(err) => SendResult::Err(err),
        }
    }
}

fn collect(ok: &SendOk<Multistatus<Prop>>) -> BTreeSet<Calendar> {
    let mut calendars = BTreeSet::new();

    let Some(responses) = &ok.body.responses else {
        return calendars;
    };

    for response in responses {
        trace!("process multistatus response");

        if let Some(status) = &response.status {
            if !status.is_success() {
                trace!("skip multistatus response with non-2xx status");
                continue;
            }
        }

        let Some(propstats) = &response.propstats else {
            continue;
        };

        let id = response
            .href
            .value
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();

        let mut calendar = Calendar {
            id,
            ..Default::default()
        };
        let mut is_calendar = false;

        for propstat in propstats {
            if !propstat.status.is_success() {
                trace!("skip propstat with non-2xx status");
                continue;
            }

            if let Some(rtype) = &propstat.prop.resourcetype {
                if rtype.calendar.is_some() {
                    is_calendar = true;
                }
            }

            if let Some(name) = non_empty(propstat.prop.displayname.as_deref()) {
                calendar.display_name = Some(name);
            }

            if let Some(desc) = non_empty(propstat.prop.calendar_description.as_deref()) {
                calendar.description = Some(desc);
            }

            if let Some(color) = non_empty(propstat.prop.calendar_color.as_deref()) {
                calendar.color = Some(color);
            }

            if let Some(ctag) = non_empty(propstat.prop.getctag.as_deref()) {
                calendar.ctag = Some(ctag);
            }

            if let Some(tz) = non_empty(propstat.prop.calendar_timezone.as_deref()) {
                calendar.tz = Some(tz);
            }
        }

        if is_calendar && !calendar.id.is_empty() {
            calendars.insert(calendar);
        }
    }

    calendars
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// `<prop>` payload returned by the list-calendars PROPFIND.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub resourcetype: Option<ResourceType>,
    pub displayname: Option<String>,
    pub calendar_color: Option<String>,
    pub calendar_description: Option<String>,
    pub getctag: Option<String>,
    pub calendar_timezone: Option<String>,
}

/// `<resourcetype>` element returned by the list-calendars PROPFIND.
#[derive(Clone, Debug, Deserialize)]
pub struct ResourceType {
    pub calendar: Option<()>,
}
