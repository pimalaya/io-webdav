//! # Calendar collections
//!
//! CalDAV calendar collections (RFC 4791 §4).
//!
//! Holds the shared [`CaldavCalendar`] type, the CalDAV property vocabulary and
//! the request-body helpers the calendar coroutines reuse. Each coroutine is
//! its own submodule.

pub mod create;
pub mod delete;
pub mod home_set;
pub mod list;
pub mod update;

use alloc::{collections::BTreeSet, format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::rfc4918::{
    DISPLAYNAME, GETCTAG, RESOURCETYPE, SUPPORTED_REPORT_SET, SYNC_TOKEN, WebdavNamespace,
    WebdavPropValue, WebdavProperty, escape_text, prop_set_body, report_query_body,
};

/// A CalDAV calendar collection (RFC 4791 §4).
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct CaldavCalendar {
    /// Calendar identifier: the last non-empty path segment of the calendar
    /// collection URL.
    pub id: String,
    /// Human-readable display name (DAV:displayname).
    pub display_name: Option<String>,
    /// Free-form description (RFC 4791 §6.2.1).
    pub description: Option<String>,
    /// Display color, expressed as a CSS hex string (RFC 7986 §5.9).
    pub color: Option<String>,
    /// Component types the calendar holds (RFC 4791 §5.2.3), e.g. `VEVENT`,
    /// `VTODO`, `VJOURNAL`, empty when the server advertises no restriction and
    /// therefore accepts any type.
    ///
    /// A server fixes this at creation time and refuses to change it
    /// afterwards, so setting it only means something on create.
    pub components: BTreeSet<String>,
    /// Collection change tag (CalendarServer ctag extension), bumped on every
    /// change to the calendar.
    pub ctag: Option<String>,
    /// Collection sync token (RFC 6578 §4), the checkpoint fed back to a
    /// `sync-collection` REPORT.
    pub sync_token: Option<String>,
    /// Default time zone, expressed as a VTIMEZONE block (RFC 4791 §5.2.2).
    pub tz: Option<String>,
    /// Reports the server advertises for this collection (RFC 3253 §3.1.5),
    /// e.g. `sync-collection`, `calendar-query`, `calendar-multiget`.
    ///
    /// RFC 6578 is an extension: a collection whose set holds no
    /// `sync-collection` is enumerated with
    /// [`WebdavSyncCollectionOptions::fallback`] instead, without paying a
    /// failed REPORT first.
    ///
    /// [`WebdavSyncCollectionOptions::fallback`]: crate::rfc6578::sync_collection::WebdavSyncCollectionOptions::fallback
    pub supported_reports: BTreeSet<String>,
}

/// CalDAV namespace (RFC 4791 §4).
pub const CALDAV: WebdavNamespace = WebdavNamespace {
    uri: "urn:ietf:params:xml:ns:caldav",
    prefix: "C",
};
/// inf-it extension namespace (calendar color).
pub const INFIT: WebdavNamespace = WebdavNamespace {
    uri: "http://inf-it.com/ns/ab/",
    prefix: "I",
};

/// `C:calendar` resourcetype marker (RFC 4791 §4.2).
pub const CALENDAR: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar",
};
/// `C:calendar-home-set` (RFC 4791 §6.2.1).
pub const CALENDAR_HOME_SET: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar-home-set",
};
/// `C:calendar-description` (RFC 4791 §5.2.1).
pub const CALENDAR_DESCRIPTION: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar-description",
};
/// `C:calendar-timezone` (RFC 4791 §5.2.2).
pub const CALENDAR_TIMEZONE: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar-timezone",
};
/// `C:calendar-data` (RFC 4791 §9.6).
pub const CALENDAR_DATA: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar-data",
};
/// `I:calendar-color` (inf-it extension).
pub const CALENDAR_COLOR: WebdavProperty = WebdavProperty {
    ns: INFIT,
    local: "calendar-color",
};
/// `C:supported-calendar-component-set` (RFC 4791 §5.2.3).
pub const SUPPORTED_CALENDAR_COMPONENT_SET: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "supported-calendar-component-set",
};
/// `C:calendar-query` REPORT root (RFC 4791 §7.8).
pub const CALENDAR_QUERY: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar-query",
};
/// `C:calendar-multiget` REPORT root (RFC 4791 §7.9).
pub const CALENDAR_MULTIGET: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar-multiget",
};
/// `C:mkcalendar` MKCALENDAR request root (RFC 4791 §5.3.1).
pub const MKCALENDAR: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "mkcalendar",
};

/// Properties requested when listing calendars.
pub const LIST_PROPS: &[WebdavProperty] = &[
    RESOURCETYPE,
    DISPLAYNAME,
    CALENDAR_DESCRIPTION,
    CALENDAR_COLOR,
    SUPPORTED_CALENDAR_COMPONENT_SET,
    GETCTAG,
    SYNC_TOKEN,
    SUPPORTED_REPORT_SET,
    CALENDAR_TIMEZONE,
];

/// Joins a home-set path with a calendar id into a collection path (trailing
/// slash included).
pub fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}

/// Builds a CalDAV `MKCALENDAR` request body (RFC 4791 §5.3.1) setting the
/// given properties, calendars requiring this dedicated method rather than the
/// extended `MKCOL` of plain collections.
pub fn mkcalendar_body(set: &[(WebdavProperty, WebdavPropValue<'_>)]) -> Vec<u8> {
    prop_set_body(MKCALENDAR, set)
}

/// Turns the display name, color, description, time zone and component set of
/// `calendar` into `PROPPATCH` or `MKCALENDAR` set pairs.
///
/// A [`None`] field is left out rather than sent empty, so a partially filled
/// [`CaldavCalendar`] patches only what it carries.
pub fn property_set(calendar: &CaldavCalendar) -> Vec<(WebdavProperty, WebdavPropValue<'_>)> {
    let mut set = Vec::new();
    if let Some(name) = &calendar.display_name {
        set.push((DISPLAYNAME, WebdavPropValue::Text(name)));
    }
    if let Some(color) = &calendar.color {
        set.push((CALENDAR_COLOR, WebdavPropValue::Text(color)));
    }
    if let Some(description) = &calendar.description {
        set.push((CALENDAR_DESCRIPTION, WebdavPropValue::Text(description)));
    }
    if let Some(tz) = &calendar.tz {
        set.push((CALENDAR_TIMEZONE, WebdavPropValue::Text(tz)));
    }
    if !calendar.components.is_empty() {
        let mut comps = String::new();
        for component in &calendar.components {
            // NOTE: an attribute value, so the double quote needs escaping on
            // top of what escape_text covers.
            let name = escape_text(component).replace('"', "&quot;");
            comps.push_str(&format!("<C:comp name=\"{name}\"/>"));
        }
        set.push((
            SUPPORTED_CALENDAR_COMPONENT_SET,
            WebdavPropValue::Raw(comps),
        ));
    }
    set
}

/// Builds a CalDAV `calendar-multiget` REPORT body (RFC 4791 §7.9) requesting
/// `props` for each given href.
pub fn calendar_multiget_body(hrefs: &[String], props: &[WebdavProperty]) -> Vec<u8> {
    let mut fragment = String::new();
    for href in hrefs {
        fragment.push_str(&format!("<D:href>{}</D:href>", escape_text(href)));
    }
    report_query_body(CALENDAR_MULTIGET, &[CALDAV], props, &fragment)
}

/// Builds a CalDAV `calendar-query` REPORT body requesting `props`.
///
/// `comp_filter` is the optional VCALENDAR child filter (e.g. `<C:comp-filter
/// name="VEVENT" />`), an empty string listing every component type.
pub fn calendar_query_body(props: &[WebdavProperty], comp_filter: &str) -> Vec<u8> {
    let filter = format!(
        "<C:filter><C:comp-filter name=\"VCALENDAR\">{comp_filter}</C:comp-filter></C:filter>"
    );
    report_query_body(CALENDAR_QUERY, &[CALDAV], props, &filter)
}
