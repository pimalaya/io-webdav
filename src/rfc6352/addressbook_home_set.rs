//! `addressbook-home-set` discovery (RFC 6352 §7.1.1).
//!
//! Runs a PROPFIND against the principal URL and surfaces the
//! discovered addressbook-home-set URL.
//!
//! Lifted from io-addressbook/src/carddav/coroutines/addressbook-home-set.rs.

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

const BODY: &str = include_str!("./addressbook_home_set.xml");

/// Result returned by [`AddressbookHomeSet::resume`].
#[derive(Debug)]
pub enum AddressbookHomeSetResult {
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

/// Coroutine that discovers the addressbook-home-set URL.
#[derive(Debug)]
pub struct AddressbookHomeSet {
    base_url: Url,
    send: FollowRedirects<Multistatus<Prop>>,
}

impl AddressbookHomeSet {
    /// Builds a new `addressbook-home-set` discovery coroutine.
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
    pub fn resume(&mut self, arg: Option<&[u8]>) -> AddressbookHomeSetResult {
        let ok = match self.send.resume(arg) {
            FollowRedirectsResult::Ok(ok) => ok,
            FollowRedirectsResult::WantsRead => return AddressbookHomeSetResult::WantsRead,
            FollowRedirectsResult::WantsWrite(bytes) => {
                return AddressbookHomeSetResult::WantsWrite(bytes);
            }
            FollowRedirectsResult::WantsRedirect {
                url,
                keep_alive,
                same_origin,
            } => {
                return AddressbookHomeSetResult::WantsRedirect {
                    url,
                    keep_alive,
                    same_origin,
                };
            }
            FollowRedirectsResult::Err(err) => return AddressbookHomeSetResult::Err(err),
        };

        let url = first_home_set(&ok, &self.base_url);

        AddressbookHomeSetResult::Ok { url, ok }
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

            if let Ok(url) = propstat.prop.addressbook_home_set.url(base_url) {
                return Some(url);
            }
        }
    }

    None
}

/// `<prop>` payload returned by the addressbook-home-set discovery
/// PROPFIND.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Prop {
    pub addressbook_home_set: HrefProp,
}
