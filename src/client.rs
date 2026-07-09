//! # Standard, blocking WebDAV client
//!
//! Holds a single boxed stream (any blocking `Read + Write` impl) plus the
//! [`WebdavAuth`] credential, the user-facing pub options ([`base_url`],
//! [`user_agent`]) and the discovery caches ([`principal_url`],
//! [`calendar_home_set`], [`addressbook_home_set`]).
//!
//! The bare [`new`] constructor takes a pre-connected stream; callers handle
//! TCP and TLS themselves. With one of the TLS feature flags enabled
//! (`rustls-ring`, `rustls-aws`, `native-tls`), [`connect`] is also available
//! and handles `https://` URLs end-to-end via
//! [`pimalaya_stream::std::stream::StreamStd`].
//!
//! Discovery flows top-down from the configured [`base_url`] (the DAV
//! context root, resolved by pimconf's RFC 6764 discovery upstream):
//! [`current_user_principal`] resolves the principal URL;
//! [`calendar_home_set`] / [`addressbook_home_set`] resolve the per-RFC
//! home-set URL. Each step caches its result; higher-level methods return
//! [`MissingPrincipal`] / [`MissingCalendarHomeSet`] /
//! [`MissingAddressbookHomeSet`] when the cache is empty (mirrors io-jmap's
//! `MissingSession`).
//!
//! [`base_url`]: WebdavClientStd::base_url
//! [`user_agent`]: WebdavClientStd::user_agent
//! [`principal_url`]: WebdavClientStd::principal_url
//! [`calendar_home_set`]: WebdavClientStd::calendar_home_set
//! [`addressbook_home_set`]: WebdavClientStd::addressbook_home_set
//! [`new`]: WebdavClientStd::new
//! [`connect`]: WebdavClientStd::connect
//! [`current_user_principal`]: WebdavClientStd::current_user_principal
//! [`MissingPrincipal`]: WebdavClientStdError::MissingPrincipal
//! [`MissingCalendarHomeSet`]: WebdavClientStdError::MissingCalendarHomeSet
//! [`MissingAddressbookHomeSet`]: WebdavClientStdError::MissingAddressbookHomeSet

use core::fmt;

use alloc::{
    boxed::Box,
    collections::BTreeSet,
    format,
    string::{String, ToString},
    vec::Vec,
};

use std::io::{self, Read, Write};

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
use pimalaya_stream::{std::stream::StreamStd, tls::Tls};
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::{
        calendar::{
            Calendar, create::CreateCalendar, delete::DeleteCalendar, home_set::CalendarHomeSet,
            list::ListCalendars, update::UpdateCalendar,
        },
        item::{
            CreateItemOk, ItemBody, ItemEntry, UpdateItemOk, create::CreateItem,
            delete::DeleteItem, list::ListItems, read::ReadItem, update::UpdateItem,
        },
    },
    rfc4918::{
        GETETAG, WebdavAuth, coroutine::WebdavRedirectYield,
        follow_redirects::FollowRedirectsError, send::SendError,
    },
    rfc5397::current_user_principal::CurrentUserPrincipal,
    rfc6352::{
        addressbook::{
            Addressbook, create::CreateAddressbook, delete::DeleteAddressbook,
            home_set::AddressbookHomeSet, list::ListAddressbooks, update::UpdateAddressbook,
        },
        card::{
            CardBody, CardEntry, CardRef, CreateCardOk, UpdateCardOk, create::CreateCard,
            delete::DeleteCard, enumerate::EnumCards, list::ListCards, multiget::MultigetCards,
            read::ReadCard, update::UpdateCard,
        },
    },
    rfc6578::sync_collection::{SyncCollection, SyncCollectionError, SyncDelta},
};

const READ_BUFFER_SIZE: usize = 16 * 1024;

const DEFAULT_USER_AGENT: &str = concat!("io-webdav/", env!("CARGO_PKG_VERSION"));

/// Errors returned by [`WebdavClientStd`].
#[derive(Debug, Error)]
pub enum WebdavClientStdError {
    #[error(transparent)]
    Send(#[from] SendError),
    #[error(transparent)]
    FollowRedirects(#[from] FollowRedirectsError),
    #[error(transparent)]
    SyncCollection(#[from] SyncCollectionError),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    #[error(transparent)]
    Tls(#[from] anyhow::Error),
    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    #[error("WebDAV URL `{0}` has no host")]
    UrlMissingHost(String),
    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    #[error("WebDAV URL `{0}` has unsupported scheme `{1}` (expected `http` or `https`)")]
    UrlUnsupportedScheme(String, String),

    #[error("WebDAV server redirected to `{0}` during a non-redirectable operation")]
    UnexpectedRedirect(Url),

    #[error("WebDAV client missing principal URL; call `current_user_principal` first")]
    MissingPrincipal,
    #[error("WebDAV client missing calendar home-set; call `calendar_home_set` first")]
    MissingCalendarHomeSet,
    #[error("WebDAV client missing addressbook home-set; call `addressbook_home_set` first")]
    MissingAddressbookHomeSet,
}

/// Std-blocking WebDAV client wrapping a single blocking stream.
pub struct WebdavClientStd {
    /// The active blocking stream. Public so higher-level crates can pump their
    /// own [`WebdavCoroutine`]s through it (as io-jmap exposes its stream),
    /// reusing this client's discovery cache.
    pub stream: Box<dyn WebdavStream>,

    auth: WebdavAuth,

    /// Base URL prepended to every request path.
    pub base_url: Url,

    /// `User-Agent` header value.
    pub user_agent: String,

    /// Cached principal URL (RFC 5397).
    pub principal_url: Option<Url>,

    /// Cached CalDAV home-set URL (RFC 4791 §6.2.1).
    pub calendar_home_set: Option<Url>,

    /// Cached CardDAV home-set URL (RFC 6352 §7.1.1).
    pub addressbook_home_set: Option<Url>,
}

impl fmt::Debug for WebdavClientStd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebdavClientStd")
            .field("base_url", &self.base_url.as_str())
            .field("user_agent", &self.user_agent)
            .field(
                "principal_url",
                &self.principal_url.as_ref().map(Url::as_str),
            )
            .field(
                "calendar_home_set",
                &self.calendar_home_set.as_ref().map(Url::as_str),
            )
            .field(
                "addressbook_home_set",
                &self.addressbook_home_set.as_ref().map(Url::as_str),
            )
            .finish_non_exhaustive()
    }
}

impl WebdavClientStd {
    /// Builds a client around `stream`. The caller is responsible for
    /// opening the connection (TCP, TLS handshake if needed).
    pub fn new<S: Read + Write + Send + 'static>(
        stream: S,
        auth: WebdavAuth,
        base_url: Url,
    ) -> Self {
        Self {
            stream: Box::new(stream),
            auth,
            base_url,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            principal_url: None,
            calendar_home_set: None,
            addressbook_home_set: None,
        }
    }

    /// Builds a client from a pre-connected stream and the full
    /// discovery state already in hand. Skips every discovery step.
    pub fn from_parts<S: Read + Write + Send + 'static>(
        stream: S,
        auth: WebdavAuth,
        base_url: Url,
        principal_url: Option<Url>,
        calendar_home_set: Option<Url>,
        addressbook_home_set: Option<Url>,
    ) -> Self {
        Self {
            stream: Box::new(stream),
            auth,
            base_url,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            principal_url,
            calendar_home_set,
            addressbook_home_set,
        }
    }

    /// Connects to `url`'s host and runs the TLS handshake when the
    /// scheme is `https`. `http` goes through plain TCP. ALPN is set
    /// to `http/1.1`.
    #[cfg(any(
        feature = "rustls-aws",
        feature = "rustls-ring",
        feature = "native-tls"
    ))]
    pub fn connect(url: &Url, tls: &Tls, auth: WebdavAuth) -> Result<Self, WebdavClientStdError> {
        let host = url
            .host_str()
            .ok_or_else(|| WebdavClientStdError::UrlMissingHost(url.to_string()))?;

        let stream = match url.scheme() {
            "http" => StreamStd::connect_tcp(host, url.port().unwrap_or(80))?,
            "https" => StreamStd::connect_tls(host, url.port().unwrap_or(443), tls)?,
            scheme => {
                return Err(WebdavClientStdError::UrlUnsupportedScheme(
                    url.to_string(),
                    scheme.to_string(),
                ));
            }
        };

        Ok(Self::new(stream, auth, url.clone()))
    }

    /// Replaces the underlying stream; useful when discovery surfaces a
    /// new authority and the caller has to reconnect.
    pub fn set_stream<S: Read + Write + Send + 'static>(&mut self, stream: S) {
        self.stream = Box::new(stream);
    }

    /// Returns the active authentication scheme.
    pub fn auth(&self) -> &WebdavAuth {
        &self.auth
    }

    /// Runs any standard-shape coroutine (`Yield = WebdavYield`)
    /// against the client stream until completion. Redirect-aware
    /// discovery uses [`run_redirect`](Self::run_redirect) instead.
    fn run<C, T, E>(&mut self, mut coroutine: C) -> Result<T, WebdavClientStdError>
    where
        C: WebdavCoroutine<Yield = WebdavYield, Return = Result<T, E>>,
        E: Into<WebdavClientStdError>,
    {
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;

        let ret = loop {
            match coroutine.resume(arg.take()) {
                WebdavCoroutineState::Complete(ret) => break ret,
                WebdavCoroutineState::Yielded(yielded) => {
                    let n = pump(&mut *self.stream, &mut buf, yielded)?;
                    arg = n.map(|n| &buf[..n]);
                }
            }
        };

        ret.map_err(Into::into)
    }

    /// Runs a redirect-aware discovery coroutine (`Yield =
    /// WebdavRedirectYield`, `Return = Option<Url>`). A 3xx is surfaced
    /// as [`UnexpectedRedirect`] rather than followed: this client owns
    /// a single connected stream, so only the caller (who owns
    /// connection creation) can reconnect to the target and retry, e.g.
    /// via [`set_stream`] (mirrors io-http's `HttpClientStd`).
    ///
    /// [`UnexpectedRedirect`]: WebdavClientStdError::UnexpectedRedirect
    /// [`set_stream`]: WebdavClientStd::set_stream
    fn run_redirect(
        &mut self,
        coroutine: &mut dyn WebdavCoroutine<
            Yield = WebdavRedirectYield,
            Return = Result<Option<Url>, FollowRedirectsError>,
        >,
    ) -> Result<Option<Url>, WebdavClientStdError> {
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;

        loop {
            match coroutine.resume(arg.take()) {
                WebdavCoroutineState::Complete(Ok(url)) => return Ok(url),
                WebdavCoroutineState::Complete(Err(err)) => return Err(err.into()),
                WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRead) => {
                    let n = self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsWrite(bytes)) => {
                    self.stream.write_all(&bytes)?;
                    arg = None;
                }
                WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRedirect {
                    url, ..
                }) => {
                    return Err(WebdavClientStdError::UnexpectedRedirect(url));
                }
            }
        }
    }

    // ---- Discovery (RFC 5397 + per-RFC home-set) ------------------------

    /// Discovers the current user principal URL (RFC 5397) and caches
    /// it in [`principal_url`]. Subsequent calls return the cached
    /// value without hitting the network.
    ///
    /// [`principal_url`]: WebdavClientStd::principal_url
    pub fn current_user_principal(&mut self) -> Result<Url, WebdavClientStdError> {
        if let Some(url) = &self.principal_url {
            return Ok(url.clone());
        }

        let mut coroutine = CurrentUserPrincipal::new(&self.base_url, &self.auth, &self.user_agent);
        let url = self.run_redirect(&mut coroutine)?;
        let url = url.ok_or(WebdavClientStdError::MissingPrincipal)?;

        self.principal_url = Some(url.clone());
        Ok(url)
    }

    // ---- CalDAV (RFC 4791) ----------------------------------------------

    /// Discovers the CalDAV home-set URL (RFC 4791 §6.2.1) and caches
    /// it in [`calendar_home_set`]. Resolves [`principal_url`] first
    /// when it is not cached.
    ///
    /// [`calendar_home_set`]: WebdavClientStd::calendar_home_set
    /// [`principal_url`]: WebdavClientStd::principal_url
    pub fn calendar_home_set(&mut self) -> Result<Url, WebdavClientStdError> {
        if let Some(url) = &self.calendar_home_set {
            return Ok(url.clone());
        }

        let principal = self.current_user_principal()?;
        let path = principal.path().to_string();

        let mut coroutine =
            CalendarHomeSet::new(&self.base_url, &self.auth, &self.user_agent, &path);
        let url = self.run_redirect(&mut coroutine)?;
        let url = url.ok_or(WebdavClientStdError::MissingCalendarHomeSet)?;

        self.calendar_home_set = Some(url.clone());
        Ok(url)
    }

    /// Lists every calendar under the cached
    /// [`calendar_home_set`].
    ///
    /// [`calendar_home_set`]: WebdavClientStd::calendar_home_set
    pub fn list_calendars(&mut self) -> Result<BTreeSet<Calendar>, WebdavClientStdError> {
        let home = self
            .calendar_home_set
            .as_ref()
            .ok_or(WebdavClientStdError::MissingCalendarHomeSet)?;
        let path = home.path().to_string();

        let coroutine = ListCalendars::new(&self.base_url, &self.auth, &self.user_agent, &path);
        self.run(coroutine)
    }

    /// Creates a calendar collection under the cached
    /// [`calendar_home_set`].
    ///
    /// [`calendar_home_set`]: WebdavClientStd::calendar_home_set
    pub fn create_calendar(&mut self, calendar: &Calendar) -> Result<(), WebdavClientStdError> {
        let home = self
            .calendar_home_set
            .as_ref()
            .ok_or(WebdavClientStdError::MissingCalendarHomeSet)?;
        let path = home.path().to_string();

        let coroutine = CreateCalendar::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            calendar,
        );
        self.run(coroutine).map(|_| ())
    }

    /// Updates a calendar collection's properties.
    pub fn update_calendar(&mut self, calendar: &Calendar) -> Result<(), WebdavClientStdError> {
        let home = self
            .calendar_home_set
            .as_ref()
            .ok_or(WebdavClientStdError::MissingCalendarHomeSet)?;
        let path = home.path().to_string();

        let coroutine = UpdateCalendar::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            calendar,
        );
        self.run(coroutine)
    }

    /// Deletes a calendar collection.
    pub fn delete_calendar(&mut self, calendar_id: &str) -> Result<(), WebdavClientStdError> {
        let home = self
            .calendar_home_set
            .as_ref()
            .ok_or(WebdavClientStdError::MissingCalendarHomeSet)?;
        let path = home.path().to_string();

        let coroutine = DeleteCalendar::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            calendar_id,
        );
        self.run(coroutine).map(|_| ())
    }

    /// Lists every iCalendar item inside `calendar_id`. `comp_filter`
    /// is the optional VCALENDAR child filter (e.g.
    /// `<C:comp-filter name=\"VEVENT\" />`); pass an empty string to
    /// list every component type.
    pub fn list_items(
        &mut self,
        calendar_id: &str,
        comp_filter: &str,
    ) -> Result<BTreeSet<ItemEntry>, WebdavClientStdError> {
        let path = calendar_path(self.calendar_home_set.as_ref(), calendar_id)?;
        let coroutine = ListItems::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            comp_filter,
        );
        self.run(coroutine)
    }

    /// Reads a single calendar item's raw iCalendar bytes plus its
    /// ETag.
    pub fn read_item(
        &mut self,
        calendar_id: &str,
        item_id: &str,
    ) -> Result<ItemBody, WebdavClientStdError> {
        let path = calendar_path(self.calendar_home_set.as_ref(), calendar_id)?;
        let coroutine = ReadItem::new(&self.base_url, &self.auth, &self.user_agent, &path, item_id);
        self.run(coroutine)
    }

    /// Creates a calendar item by id.
    pub fn create_item(
        &mut self,
        calendar_id: &str,
        id: &str,
        ical: Vec<u8>,
    ) -> Result<CreateItemOk, WebdavClientStdError> {
        let path = calendar_path(self.calendar_home_set.as_ref(), calendar_id)?;
        let coroutine = CreateItem::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            id,
            ical,
        );
        self.run(coroutine)
    }

    /// Updates an existing calendar item.
    pub fn update_item(
        &mut self,
        calendar_id: &str,
        id: &str,
        ical: Vec<u8>,
        if_match: Option<&str>,
    ) -> Result<UpdateItemOk, WebdavClientStdError> {
        let path = calendar_path(self.calendar_home_set.as_ref(), calendar_id)?;
        let coroutine = UpdateItem::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            id,
            ical,
            if_match,
        );
        self.run(coroutine)
    }

    /// Deletes a calendar item.
    pub fn delete_item(
        &mut self,
        calendar_id: &str,
        item_id: &str,
        if_match: Option<&str>,
    ) -> Result<(), WebdavClientStdError> {
        let path = calendar_path(self.calendar_home_set.as_ref(), calendar_id)?;
        let coroutine = DeleteItem::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            item_id,
            if_match,
        );
        self.run(coroutine).map(|_| ())
    }

    // ---- CardDAV (RFC 6352) ---------------------------------------------

    /// Discovers the CardDAV home-set URL (RFC 6352 §7.1.1) and caches
    /// it in [`addressbook_home_set`].
    ///
    /// [`addressbook_home_set`]: WebdavClientStd::addressbook_home_set
    pub fn addressbook_home_set(&mut self) -> Result<Url, WebdavClientStdError> {
        if let Some(url) = &self.addressbook_home_set {
            return Ok(url.clone());
        }

        let principal = self.current_user_principal()?;
        let path = principal.path().to_string();

        let mut coroutine =
            AddressbookHomeSet::new(&self.base_url, &self.auth, &self.user_agent, &path);
        let url = self.run_redirect(&mut coroutine)?;
        let url = url.ok_or(WebdavClientStdError::MissingAddressbookHomeSet)?;

        self.addressbook_home_set = Some(url.clone());
        Ok(url)
    }

    /// Lists every addressbook under the cached
    /// [`addressbook_home_set`].
    ///
    /// [`addressbook_home_set`]: WebdavClientStd::addressbook_home_set
    pub fn list_addressbooks(&mut self) -> Result<BTreeSet<Addressbook>, WebdavClientStdError> {
        let home = self
            .addressbook_home_set
            .as_ref()
            .ok_or(WebdavClientStdError::MissingAddressbookHomeSet)?;
        let path = home.path().to_string();

        let coroutine = ListAddressbooks::new(&self.base_url, &self.auth, &self.user_agent, &path);
        self.run(coroutine)
    }

    /// Creates an addressbook collection under the cached
    /// [`addressbook_home_set`].
    ///
    /// [`addressbook_home_set`]: WebdavClientStd::addressbook_home_set
    pub fn create_addressbook(
        &mut self,
        addressbook: &Addressbook,
    ) -> Result<(), WebdavClientStdError> {
        let home = self
            .addressbook_home_set
            .as_ref()
            .ok_or(WebdavClientStdError::MissingAddressbookHomeSet)?;
        let path = home.path().to_string();

        let coroutine = CreateAddressbook::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            addressbook,
        );
        self.run(coroutine).map(|_| ())
    }

    /// Updates an addressbook collection's properties.
    pub fn update_addressbook(
        &mut self,
        addressbook: &Addressbook,
    ) -> Result<(), WebdavClientStdError> {
        let home = self
            .addressbook_home_set
            .as_ref()
            .ok_or(WebdavClientStdError::MissingAddressbookHomeSet)?;
        let path = home.path().to_string();

        let coroutine = UpdateAddressbook::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            addressbook,
        );
        self.run(coroutine)
    }

    /// Deletes an addressbook collection.
    pub fn delete_addressbook(&mut self, addressbook_id: &str) -> Result<(), WebdavClientStdError> {
        let home = self
            .addressbook_home_set
            .as_ref()
            .ok_or(WebdavClientStdError::MissingAddressbookHomeSet)?;
        let path = home.path().to_string();

        let coroutine = DeleteAddressbook::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            addressbook_id,
        );
        self.run(coroutine).map(|_| ())
    }

    /// Lists every card inside `addressbook_id`.
    pub fn list_cards(
        &mut self,
        addressbook_id: &str,
    ) -> Result<BTreeSet<CardEntry>, WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine = ListCards::new(&self.base_url, &self.auth, &self.user_agent, &path);
        self.run(coroutine)
    }

    /// Enumerates card references (id plus ETag, no bodies) inside
    /// `addressbook_id`.
    pub fn enum_cards(
        &mut self,
        addressbook_id: &str,
    ) -> Result<BTreeSet<CardRef>, WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine = EnumCards::new(&self.base_url, &self.auth, &self.user_agent, &path);
        self.run(coroutine)
    }

    /// Batch-fetches cards by id inside `addressbook_id` in a single
    /// round-trip.
    pub fn multiget_cards(
        &mut self,
        addressbook_id: &str,
        ids: &[&str],
    ) -> Result<Vec<CardEntry>, WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine =
            MultigetCards::new(&self.base_url, &self.auth, &self.user_agent, &path, ids);
        self.run(coroutine)
    }

    /// Runs an incremental `sync-collection` REPORT (RFC 6578) against
    /// `addressbook_id`, requesting ETags only. Pass [`None`] as
    /// `sync_token` for an initial sync.
    pub fn sync_cards(
        &mut self,
        addressbook_id: &str,
        sync_token: Option<&str>,
    ) -> Result<SyncDelta, WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine = SyncCollection::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            sync_token,
            &[GETETAG],
        );
        self.run(coroutine)
    }

    /// Reads a single card's raw vCard bytes plus its ETag.
    pub fn read_card(
        &mut self,
        addressbook_id: &str,
        card_id: &str,
    ) -> Result<CardBody, WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine = ReadCard::new(&self.base_url, &self.auth, &self.user_agent, &path, card_id);
        self.run(coroutine)
    }

    /// Creates a card by id.
    pub fn create_card(
        &mut self,
        addressbook_id: &str,
        id: &str,
        vcard: Vec<u8>,
    ) -> Result<CreateCardOk, WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine = CreateCard::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            id,
            vcard,
        );
        self.run(coroutine)
    }

    /// Updates an existing card.
    pub fn update_card(
        &mut self,
        addressbook_id: &str,
        id: &str,
        vcard: Vec<u8>,
        if_match: Option<&str>,
    ) -> Result<UpdateCardOk, WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine = UpdateCard::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            id,
            vcard,
            if_match,
        );
        self.run(coroutine)
    }

    /// Deletes a card.
    pub fn delete_card(
        &mut self,
        addressbook_id: &str,
        card_id: &str,
        if_match: Option<&str>,
    ) -> Result<(), WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine = DeleteCard::new(
            &self.base_url,
            &self.auth,
            &self.user_agent,
            &path,
            card_id,
            if_match,
        );
        self.run(coroutine).map(|_| ())
    }
}

/// Runs one standard I/O yield against the stream: writes the bytes, or
/// reads a chunk into `buf` and returns its length.
fn pump(
    stream: &mut dyn WebdavStream,
    buf: &mut [u8],
    yielded: WebdavYield,
) -> Result<Option<usize>, io::Error> {
    match yielded {
        WebdavYield::WantsRead => Ok(Some(stream.read(buf)?)),
        WebdavYield::WantsWrite(bytes) => {
            stream.write_all(&bytes)?;
            Ok(None)
        }
    }
}

fn calendar_path(home: Option<&Url>, calendar_id: &str) -> Result<String, WebdavClientStdError> {
    let home = home.ok_or(WebdavClientStdError::MissingCalendarHomeSet)?;
    let base = home.path().trim_end_matches('/');
    let id = calendar_id.trim_matches('/');
    Ok(format!("{base}/{id}"))
}

fn addressbook_path(
    home: Option<&Url>,
    addressbook_id: &str,
) -> Result<String, WebdavClientStdError> {
    let home = home.ok_or(WebdavClientStdError::MissingAddressbookHomeSet)?;
    let base = home.path().trim_end_matches('/');
    let id = addressbook_id.trim_matches('/');
    Ok(format!("{base}/{id}"))
}

/// Marker for everything the client can run against; auto-implemented
/// for any blocking `Read + Write + Send` impl.
pub trait WebdavStream: Read + Write + Send {}
impl<T: Read + Write + Send + ?Sized> WebdavStream for T {}
