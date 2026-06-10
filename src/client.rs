//! # Standard, blocking WebDAV client
//!
//! Holds a single boxed stream (any blocking `Read + Write` impl)
//! plus the [`WebdavAuth`] credential, the user-facing pub options
//! ([`base_url`], [`follow_redirects`], [`max_redirects`],
//! [`user_agent`]) and the discovery caches ([`principal_url`],
//! [`calendar_home_set`], [`addressbook_home_set`]).
//!
//! The bare [`new`] constructor takes a pre-connected stream; callers
//! handle TCP and TLS themselves. With one of the TLS feature flags
//! enabled (`rustls-ring`, `rustls-aws`, `native-tls`), [`connect`] is
//! also available and handles `https://` URLs end-to-end via
//! [`pimalaya_stream::std::stream::StreamStd`].
//!
//! Discovery flows top-down: [`well_known_caldav`] /
//! [`well_known_carddav`] resolve the DAV root; [`current_user_principal`]
//! resolves the principal URL; [`calendar_home_set`] /
//! [`addressbook_home_set`] resolve the per-RFC home-set URL. Each
//! step caches its result; higher-level methods return
//! [`MissingPrincipal`] / [`MissingCalendarHomeSet`] /
//! [`MissingAddressbookHomeSet`] when the cache is empty (mirrors
//! io-jmap's `MissingSession`).
//!
//! [`base_url`]: WebdavClientStd::base_url
//! [`follow_redirects`]: WebdavClientStd::follow_redirects
//! [`max_redirects`]: WebdavClientStd::max_redirects
//! [`user_agent`]: WebdavClientStd::user_agent
//! [`principal_url`]: WebdavClientStd::principal_url
//! [`calendar_home_set`]: WebdavClientStd::calendar_home_set
//! [`addressbook_home_set`]: WebdavClientStd::addressbook_home_set
//! [`new`]: WebdavClientStd::new
//! [`connect`]: WebdavClientStd::connect
//! [`well_known_caldav`]: WebdavClientStd::well_known_caldav
//! [`well_known_carddav`]: WebdavClientStd::well_known_carddav
//! [`current_user_principal`]: WebdavClientStd::current_user_principal
//! [`MissingPrincipal`]: WebdavClientStdError::MissingPrincipal
//! [`MissingCalendarHomeSet`]: WebdavClientStdError::MissingCalendarHomeSet
//! [`MissingAddressbookHomeSet`]: WebdavClientStdError::MissingAddressbookHomeSet

use core::fmt;

use alloc::{
    boxed::Box,
    string::{String, ToString},
};
#[cfg(any(feature = "rfc4791", feature = "rfc6352"))]
use alloc::{collections::BTreeSet, vec::Vec};
use std::io::{self, Read, Write};

use log::trace;
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
    rfc4918::{
        WebdavAuth, coroutine::WebdavRedirectYield, follow_redirects::FollowRedirectsError,
        send::SendError,
    },
    rfc5397::current_user_principal::CurrentUserPrincipal,
    rfc6764::well_known::{WellKnown, WellKnownError, WellKnownKind},
};

#[cfg(feature = "rfc4791")]
use crate::rfc4791::{
    calendar::{
        Calendar, create::CreateCalendar, delete::DeleteCalendar, home_set::CalendarHomeSet,
        list::ListCalendars, update::UpdateCalendar,
    },
    item::{
        CreateItemOk, ItemBody, ItemEntry, UpdateItemOk, create::CreateItem, delete::DeleteItem,
        list::ListItems, read::ReadItem, update::UpdateItem,
    },
};

#[cfg(feature = "rfc6352")]
use crate::rfc6352::{
    addressbook::{
        Addressbook, create::CreateAddressbook, delete::DeleteAddressbook,
        home_set::AddressbookHomeSet, list::ListAddressbooks, update::UpdateAddressbook,
    },
    card::{
        CardBody, CardEntry, CreateCardOk, UpdateCardOk, create::CreateCard, delete::DeleteCard,
        list::ListCards, read::ReadCard, update::UpdateCard,
    },
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
    WellKnown(#[from] WellKnownError),

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

    #[error("WebDAV server redirected during a non-redirectable operation")]
    UnexpectedRedirect,
    #[error("Exceeded the maximum number of redirects ({0})")]
    TooManyRedirects(u8),

    #[error("WebDAV client missing principal URL; call `current_user_principal` first")]
    MissingPrincipal,
    #[error("WebDAV client missing calendar home-set; call `calendar_home_set` first")]
    MissingCalendarHomeSet,
    #[error("WebDAV client missing addressbook home-set; call `addressbook_home_set` first")]
    MissingAddressbookHomeSet,
}

/// Marker for everything the client can run against; auto-implemented
/// for any blocking `Read + Write + Send` impl.
trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send + ?Sized> Stream for T {}

/// Std-blocking WebDAV client wrapping a single blocking stream.
pub struct WebdavClientStd {
    stream: Box<dyn Stream>,
    auth: WebdavAuth,

    /// Base URL prepended to every request path.
    pub base_url: Url,

    /// Whether to transparently follow 3xx redirects during discovery
    /// (defaults to `true`).
    pub follow_redirects: bool,

    /// Maximum number of redirects to follow before bailing with
    /// [`WebdavClientStdError::TooManyRedirects`].
    pub max_redirects: u8,

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
            .field("follow_redirects", &self.follow_redirects)
            .field("max_redirects", &self.max_redirects)
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
            follow_redirects: true,
            max_redirects: 5,
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
            follow_redirects: true,
            max_redirects: 5,
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

        loop {
            match coroutine.resume(arg.take()) {
                WebdavCoroutineState::Complete(Ok(out)) => return Ok(out),
                WebdavCoroutineState::Complete(Err(err)) => return Err(err.into()),
                WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
                    let n = self.stream.read(&mut buf)?;
                    arg = Some(&buf[..n]);
                }
                WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
                    self.stream.write_all(&bytes)?;
                    arg = None;
                }
            }
        }
    }

    /// Runs a redirect-aware discovery coroutine
    /// (`Yield = WebdavRedirectYield`, `Return = Option<Url>`). A 3xx
    /// is surfaced as [`UnexpectedRedirect`] (or [`TooManyRedirects`]
    /// past [`max_redirects`]) rather than followed.
    ///
    /// [`UnexpectedRedirect`]: WebdavClientStdError::UnexpectedRedirect
    /// [`TooManyRedirects`]: WebdavClientStdError::TooManyRedirects
    /// [`max_redirects`]: WebdavClientStd::max_redirects
    fn run_redirect<C>(&mut self, mut coroutine: C) -> Result<Option<Url>, WebdavClientStdError>
    where
        C: WebdavCoroutine<
                Yield = WebdavRedirectYield,
                Return = Result<Option<Url>, FollowRedirectsError>,
            >,
    {
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut arg: Option<&[u8]> = None;
        let mut redirects = 0u8;

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
                WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRedirect { .. }) => {
                    redirects += 1;
                    if redirects > self.max_redirects {
                        return Err(WebdavClientStdError::TooManyRedirects(self.max_redirects));
                    }
                    return Err(WebdavClientStdError::UnexpectedRedirect);
                }
            }
        }
    }

    // ---- Discovery (RFC 6764 + RFC 5397 + per-RFC home-set) -------------

    /// Runs RFC 6764 `.well-known/caldav` discovery and returns the
    /// redirect target URL. Does not mutate [`base_url`]; the caller
    /// decides whether to rebuild the client against the new authority.
    ///
    /// [`base_url`]: WebdavClientStd::base_url
    pub fn well_known_caldav(&mut self) -> Result<Url, WebdavClientStdError> {
        self.run_well_known(WellKnownKind::Caldav)
    }

    /// Runs RFC 6764 `.well-known/carddav` discovery.
    pub fn well_known_carddav(&mut self) -> Result<Url, WebdavClientStdError> {
        self.run_well_known(WellKnownKind::Carddav)
    }

    fn run_well_known(&mut self, kind: WellKnownKind) -> Result<Url, WebdavClientStdError> {
        trace!("resolve well-known {kind:?}");
        let coroutine = WellKnown::new(&self.base_url, &self.auth, &self.user_agent, kind);
        let out = self.run(coroutine)?;
        Ok(out.url)
    }

    /// Discovers the current user principal URL (RFC 5397) and caches
    /// it in [`principal_url`]. Subsequent calls return the cached
    /// value without hitting the network.
    ///
    /// [`principal_url`]: WebdavClientStd::principal_url
    pub fn current_user_principal(&mut self) -> Result<Url, WebdavClientStdError> {
        if let Some(url) = &self.principal_url {
            return Ok(url.clone());
        }

        let coroutine = CurrentUserPrincipal::new(&self.base_url, &self.auth, &self.user_agent);
        let url = self.run_redirect(coroutine)?;
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
    #[cfg(feature = "rfc4791")]
    pub fn calendar_home_set(&mut self) -> Result<Url, WebdavClientStdError> {
        if let Some(url) = &self.calendar_home_set {
            return Ok(url.clone());
        }

        let principal = self.current_user_principal()?;
        let path = principal.path().to_string();

        let coroutine = CalendarHomeSet::new(&self.base_url, &self.auth, &self.user_agent, &path);
        let url = self.run_redirect(coroutine)?;
        let url = url.ok_or(WebdavClientStdError::MissingCalendarHomeSet)?;

        self.calendar_home_set = Some(url.clone());
        Ok(url)
    }

    /// Lists every calendar under the cached
    /// [`calendar_home_set`].
    ///
    /// [`calendar_home_set`]: WebdavClientStd::calendar_home_set
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc4791")]
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
    #[cfg(feature = "rfc6352")]
    pub fn addressbook_home_set(&mut self) -> Result<Url, WebdavClientStdError> {
        if let Some(url) = &self.addressbook_home_set {
            return Ok(url.clone());
        }

        let principal = self.current_user_principal()?;
        let path = principal.path().to_string();

        let coroutine =
            AddressbookHomeSet::new(&self.base_url, &self.auth, &self.user_agent, &path);
        let url = self.run_redirect(coroutine)?;
        let url = url.ok_or(WebdavClientStdError::MissingAddressbookHomeSet)?;

        self.addressbook_home_set = Some(url.clone());
        Ok(url)
    }

    /// Lists every addressbook under the cached
    /// [`addressbook_home_set`].
    ///
    /// [`addressbook_home_set`]: WebdavClientStd::addressbook_home_set
    #[cfg(feature = "rfc6352")]
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
    #[cfg(feature = "rfc6352")]
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
    #[cfg(feature = "rfc6352")]
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
    #[cfg(feature = "rfc6352")]
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
    #[cfg(feature = "rfc6352")]
    pub fn list_cards(
        &mut self,
        addressbook_id: &str,
    ) -> Result<BTreeSet<CardEntry>, WebdavClientStdError> {
        let path = addressbook_path(self.addressbook_home_set.as_ref(), addressbook_id)?;
        let coroutine = ListCards::new(&self.base_url, &self.auth, &self.user_agent, &path);
        self.run(coroutine)
    }

    /// Reads a single card's raw vCard bytes plus its ETag.
    #[cfg(feature = "rfc6352")]
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
    #[cfg(feature = "rfc6352")]
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
    #[cfg(feature = "rfc6352")]
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
    #[cfg(feature = "rfc6352")]
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

#[cfg(feature = "rfc4791")]
fn calendar_path(home: Option<&Url>, calendar_id: &str) -> Result<String, WebdavClientStdError> {
    let home = home.ok_or(WebdavClientStdError::MissingCalendarHomeSet)?;
    let base = home.path().trim_end_matches('/');
    let id = calendar_id.trim_matches('/');
    Ok(alloc::format!("{base}/{id}"))
}

#[cfg(feature = "rfc6352")]
fn addressbook_path(
    home: Option<&Url>,
    addressbook_id: &str,
) -> Result<String, WebdavClientStdError> {
    let home = home.ok_or(WebdavClientStdError::MissingAddressbookHomeSet)?;
    let base = home.path().trim_end_matches('/');
    let id = addressbook_id.trim_matches('/');
    Ok(alloc::format!("{base}/{id}"))
}
