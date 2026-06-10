//! CalDAV calendar helpers shared across the create/update/delete
//! coroutines: collection path composition and request-body templating.

use alloc::{format, string::String};

use crate::rfc4791::calendar::types::Calendar;

/// Joins a home-set path with a calendar id into a collection path
/// (trailing slash included, RFC 4791 §4).
pub fn join_path(home: &str, id: &str) -> String {
    let home = home.trim_end_matches('/');
    let id = id.trim_start_matches('/');
    format!("{home}/{id}/")
}

/// Fills a `MKCOL` / `PROPPATCH` body `template` with the calendar's
/// display name, color and description fragments. Each `{}` placeholder
/// is replaced once, in that order.
pub fn format_body(template: &str, calendar: &Calendar) -> String {
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

    template
        .replacen("{}", &name, 1)
        .replacen("{}", &color, 1)
        .replacen("{}", &description, 1)
}
