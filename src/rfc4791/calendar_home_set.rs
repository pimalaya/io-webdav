//! `calendar-home-set` discovery (RFC 4791 §6.2.1).
//!
//! Runs a PROPFIND against the principal URL and surfaces the
//! discovered calendar-home-set URL.
//!
//! Lifted from io-calendar/src/caldav/coroutines/calendar-home-set.rs.

use alloc::vec::Vec;

use log::trace;
use serde::Deserialize;
use url::Url;

use crate::rfc4918::{
    auth::WebdavAuth,
    follow_redirects::{FollowRedirects, FollowRedirectsResult},
    request::WebdavRequest,
    response::{HrefProp, Multistatus},
    send::SendOk,
};

const BODY: &str = include_str!("./calendar_home_set.xml");

/// Result returned by [`CalendarHomeSet::resume`].
#[derive(Debug)]
pub enum CalendarHomeSetResult {
    /// The coroutine has successfully terminated.
    Ok {
        url: Option<Url>,
        ok: SendOk<Multistatus<Prop>>,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The server responded with a 3xx redirect.
    WantsRedirect {
        url: Url,
        keep_alive: bool,
        same_origin: bool,
    },
    /// The coroutine encountered an error.
    Err(crate::rfc4918::follow_redirects::FollowRedirectsError),
}

/// Coroutine that discovers the calendar-home-set URL.
#[derive(Debug)]
pub struct CalendarHomeSet {
    base_url: Url,
    send: FollowRedirects<Multistatus<Prop>>,
}

impl CalendarHomeSet {
    /// Builds a new `calendar-home-set` discovery coroutine targeting
    /// `principal_path` (relative to `base_url`).
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        principal_path: &str,
    ) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, principal_path)
            .content_type_xml()
            .body(BODY.as_bytes().to_vec());

        Self {
            base_url: base_url.clone(),
            send: FollowRedirects::new(request),
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> CalendarHomeSetResult {
        let ok = match self.send.resume(arg) {
            FollowRedirectsResult::Ok(ok) => ok,
            FollowRedirectsResult::WantsRead => return CalendarHomeSetResult::WantsRead,
            FollowRedirectsResult::WantsWrite(bytes) => {
                return CalendarHomeSetResult::WantsWrite(bytes);
            }
            FollowRedirectsResult::WantsRedirect {
                url,
                keep_alive,
                same_origin,
            } => {
                return CalendarHomeSetResult::WantsRedirect {
                    url,
                    keep_alive,
                    same_origin,
                };
            }
            FollowRedirectsResult::Err(err) => return CalendarHomeSetResult::Err(err),
        };

        let url = first_home_set(&ok, &self.base_url);

        CalendarHomeSetResult::Ok { url, ok }
    }
}

fn first_home_set(ok: &SendOk<Multistatus<Prop>>, base_url: &Url) -> Option<Url> {
    let responses = ok.body.responses.as_ref()?;

    for response in responses {
        if let Some(status) = &response.status {
            if !status.is_success() {
                trace!("skip multistatus response with non-2xx status");
                continue;
            }
        }

        let Some(propstats) = &response.propstats else {
            continue;
        };

        for propstat in propstats {
            if !propstat.status.is_success() {
                trace!("skip propstat with non-2xx status");
                continue;
            }

            if let Ok(url) = propstat.prop.calendar_home_set.url(base_url) {
                return Some(url);
            }
        }
    }

    None
}

/// `<prop>` payload returned by the calendar-home-set discovery
/// PROPFIND.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub calendar_home_set: HrefProp,
}
