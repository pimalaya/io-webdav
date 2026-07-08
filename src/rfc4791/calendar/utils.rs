//! CalDAV calendar vocabulary (RFC 4791 §5, §6) plus the request-body
//! helpers shared across the calendar/item coroutines.

use alloc::{format, string::String, vec::Vec};

use crate::{
    rfc4791::calendar::types::Calendar,
    rfc4918::{
        DISPLAYNAME, GETCTAG, Namespace, Property, RESOURCETYPE, prop_set_body, report_query_body,
    },
};

/// CalDAV namespace (RFC 4791 §4).
pub const CALDAV: Namespace = Namespace {
    uri: "urn:ietf:params:xml:ns:caldav",
    prefix: "C",
};
/// inf-it extension namespace (calendar color).
pub const INFIT: Namespace = Namespace {
    uri: "http://inf-it.com/ns/ab/",
    prefix: "I",
};

/// `C:calendar` resourcetype marker (RFC 4791 §4.2).
pub const CALENDAR: Property = Property {
    ns: CALDAV,
    local: "calendar",
};
/// `C:calendar-home-set` (RFC 4791 §6.2.1).
pub const CALENDAR_HOME_SET: Property = Property {
    ns: CALDAV,
    local: "calendar-home-set",
};
/// `C:calendar-description` (RFC 4791 §5.2.1).
pub const CALENDAR_DESCRIPTION: Property = Property {
    ns: CALDAV,
    local: "calendar-description",
};
/// `C:calendar-timezone` (RFC 4791 §5.2.2).
pub const CALENDAR_TIMEZONE: Property = Property {
    ns: CALDAV,
    local: "calendar-timezone",
};
/// `C:calendar-data` (RFC 4791 §9.6).
pub const CALENDAR_DATA: Property = Property {
    ns: CALDAV,
    local: "calendar-data",
};
/// `I:calendar-color` (inf-it extension).
pub const CALENDAR_COLOR: Property = Property {
    ns: INFIT,
    local: "calendar-color",
};
/// `C:calendar-query` REPORT root (RFC 4791 §7.8).
pub const CALENDAR_QUERY: Property = Property {
    ns: CALDAV,
    local: "calendar-query",
};
/// `C:mkcalendar` MKCALENDAR request root (RFC 4791 §5.3.1).
pub const MKCALENDAR: Property = Property {
    ns: CALDAV,
    local: "mkcalendar",
};

/// Properties requested when listing calendars.
pub const LIST_PROPS: &[Property] = &[
    RESOURCETYPE,
    DISPLAYNAME,
    CALENDAR_DESCRIPTION,
    CALENDAR_COLOR,
    GETCTAG,
    CALENDAR_TIMEZONE,
];

/// Joins a home-set path with a calendar id into a collection path
/// (trailing slash included).
pub fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}

/// Builds a CalDAV `MKCALENDAR` request body (RFC 4791 §5.3.1) setting
/// the given properties. CalDAV servers require this dedicated method
/// for calendars rather than the extended `MKCOL` used for plain
/// collections.
pub fn mkcalendar_body(set: &[(Property, &str)]) -> Vec<u8> {
    prop_set_body(MKCALENDAR, set)
}

/// The present display name / color / description of `calendar` as
/// `PROPPATCH` / `MKCALENDAR` set pairs.
pub fn property_set(calendar: &Calendar) -> Vec<(Property, &str)> {
    let mut set = Vec::new();
    if let Some(name) = &calendar.display_name {
        set.push((DISPLAYNAME, name.as_str()));
    }
    if let Some(color) = &calendar.color {
        set.push((CALENDAR_COLOR, color.as_str()));
    }
    if let Some(description) = &calendar.description {
        set.push((CALENDAR_DESCRIPTION, description.as_str()));
    }
    set
}

/// Builds a CalDAV `calendar-query` REPORT body requesting `props`.
///
/// `comp_filter` is the optional VCALENDAR child filter (e.g.
/// `<C:comp-filter name="VEVENT" />`); pass an empty string to list
/// every component type.
pub fn calendar_query_body(props: &[Property], comp_filter: &str) -> Vec<u8> {
    let filter = format!(
        "<C:filter><C:comp-filter name=\"VCALENDAR\">{comp_filter}</C:comp-filter></C:filter>"
    );
    report_query_body(CALENDAR_QUERY, &[CALDAV], props, &filter)
}
