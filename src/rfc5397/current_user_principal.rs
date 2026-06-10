//! `current-user-principal` discovery (RFC 5397).
//!
//! Runs a `PROPFIND` against `/` with the `<DAV:current-user-principal>`
//! property request and surfaces the discovered principal URL.
//! Follows redirects since the entry path is usually `/` and servers
//! often redirect to the actual DAV root.
//!
//! Lifted from io-calendar/src/caldav/coroutines/current-user-principal.rs
//! and io-addressbook/src/carddav/coroutines/current-user-principal.rs
//! and split out of CalDAV/CardDAV into its own RFC module.

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

const BODY: &str = include_str!("./current_user_principal.xml");

/// Result returned by [`CurrentUserPrincipal::resume`].
#[derive(Debug)]
pub enum CurrentUserPrincipalResult {
    /// The coroutine has successfully terminated. `url` is the
    /// principal URL when found, [`None`] when the server returned an
    /// empty multistatus.
    Ok {
        url: Option<Url>,
        ok: SendOk<Multistatus<Prop>>,
    },
    /// The coroutine needs more bytes to be read from the socket.
    WantsRead,
    /// The coroutine wants the given bytes to be written to the socket.
    WantsWrite(Vec<u8>),
    /// The server responded with a 3xx redirect; the caller must
    /// reconnect to `url` and retry.
    WantsRedirect {
        url: Url,
        keep_alive: bool,
        same_origin: bool,
    },
    /// The coroutine encountered an error.
    Err(crate::rfc4918::follow_redirects::FollowRedirectsError),
}

/// Coroutine that discovers the current user principal URL.
#[derive(Debug)]
pub struct CurrentUserPrincipal {
    base_url: Url,
    send: FollowRedirects<Multistatus<Prop>>,
}

impl CurrentUserPrincipal {
    /// Builds a new `current-user-principal` coroutine targeting `/`
    /// against `base_url`.
    pub fn new(base_url: &Url, auth: &WebdavAuth, user_agent: &str) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, "/")
            .content_type_xml()
            .body(BODY.as_bytes().to_vec());

        Self {
            base_url: base_url.clone(),
            send: FollowRedirects::new(request),
        }
    }

    /// Advances the coroutine.
    pub fn resume(&mut self, arg: Option<&[u8]>) -> CurrentUserPrincipalResult {
        let ok = match self.send.resume(arg) {
            FollowRedirectsResult::Ok(ok) => ok,
            FollowRedirectsResult::WantsRead => return CurrentUserPrincipalResult::WantsRead,
            FollowRedirectsResult::WantsWrite(bytes) => {
                return CurrentUserPrincipalResult::WantsWrite(bytes);
            }
            FollowRedirectsResult::WantsRedirect {
                url,
                keep_alive,
                same_origin,
            } => {
                return CurrentUserPrincipalResult::WantsRedirect {
                    url,
                    keep_alive,
                    same_origin,
                };
            }
            FollowRedirectsResult::Err(err) => return CurrentUserPrincipalResult::Err(err),
        };

        let url = first_principal(&ok, &self.base_url);

        CurrentUserPrincipalResult::Ok { url, ok }
    }
}

fn first_principal(ok: &SendOk<Multistatus<Prop>>, base_url: &Url) -> Option<Url> {
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

            if let Ok(url) = propstat.prop.current_user_principal.url(base_url) {
                return Some(url);
            }
        }
    }

    None
}

/// `<prop>` payload returned by the principal discovery PROPFIND.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub current_user_principal: HrefProp,
}
