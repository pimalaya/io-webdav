//! # What a consumer gets
//!
//! The crate's mappers before the change (`item_from_entry`, enumerate's
//! `from_entry`, calendar list's `from_entry`) over the old untyped entries,
//! and the same outputs read from typed properties by the current crate. The
//! two sides build the old crate's output types, so they compare as values.

use std::collections::BTreeMap;

use io_webdav::rfc4918::{
    collection_id,
    typed::{WebdavResponseOf, parse_multistatus_as},
};
use io_webdav_old::{
    rfc4791::{
        calendar::{
            CALENDAR, CALENDAR_COLOR, CALENDAR_DATA, CALENDAR_DESCRIPTION, CALENDAR_TIMEZONE,
            CaldavCalendar, LIST_PROPS, SUPPORTED_CALENDAR_COMPONENT_SET,
        },
        item::{CaldavItemEntry, CaldavItemRef},
    },
    rfc4918::{
        DISPLAYNAME, GETCTAG, GETETAG, RESOURCETYPE, SYNC_TOKEN, WebdavMultistatus,
        WebdavResponseEntry, trace_unrecognized,
    },
};
use serde::Deserialize;

pub const HOME: &str = "/calendars/alice/";

/// A consumer output, in the old crate's types on both sides.
#[derive(Debug, PartialEq)]
pub enum Out {
    Items(Vec<CaldavItemEntry>),
    Refs(Vec<CaldavItemRef>, Vec<String>, Option<String>),
    Calendars(Vec<CaldavCalendar>),
}

// --- before the change ---

pub fn old_items(ms: WebdavMultistatus) -> Out {
    Out::Items(ms.responses.iter().filter_map(old_item).collect())
}

fn old_item(entry: &WebdavResponseEntry) -> Option<CaldavItemEntry> {
    if entry.href.ends_with('/') {
        return None;
    }
    let id = entry.id();
    if id.is_empty() {
        return None;
    }
    let data = entry.text(CALENDAR_DATA)?;
    trace_unrecognized(entry, &[GETETAG, CALENDAR_DATA]);
    Some(CaldavItemEntry {
        id: id.to_string(),
        etag: entry
            .text(GETETAG)
            .map(|raw| raw.trim_matches('"').to_string()),
        data: data.as_bytes().to_vec(),
    })
}

pub fn old_refs(ms: WebdavMultistatus) -> Out {
    let mut refs = Vec::new();
    let mut removed = Vec::new();
    for entry in &ms.responses {
        if entry.href.ends_with('/') || entry.id().is_empty() {
            continue;
        }
        if entry.status == Some(404) {
            removed.push(entry.id().to_string());
            continue;
        }
        refs.push(CaldavItemRef {
            id: entry.id().to_string(),
            etag: entry
                .text(GETETAG)
                .map(|raw| raw.trim_matches('"').to_string()),
        });
    }
    Out::Refs(refs, removed, ms.sync_token)
}

pub fn old_calendars(ms: WebdavMultistatus) -> Out {
    Out::Calendars(ms.responses.iter().filter_map(old_calendar).collect())
}

fn old_calendar(entry: &WebdavResponseEntry) -> Option<CaldavCalendar> {
    if !entry.has_resource_type(RESOURCETYPE, CALENDAR) {
        return None;
    }
    let id = entry.collection_id(HOME);
    if id.is_empty() {
        return None;
    }
    trace_unrecognized(entry, LIST_PROPS);
    let components = entry
        .prop(SUPPORTED_CALENDAR_COMPONENT_SET)
        .map(|item| {
            item.children
                .iter()
                .filter_map(|child| child.name.clone())
                .collect()
        })
        .unwrap_or_default();
    Some(CaldavCalendar {
        id,
        display_name: entry.text(DISPLAYNAME).map(ToString::to_string),
        description: entry.text(CALENDAR_DESCRIPTION).map(ToString::to_string),
        color: entry.text(CALENDAR_COLOR).map(ToString::to_string),
        components,
        ctag: entry.text(GETCTAG).map(ToString::to_string),
        sync_token: entry.text(SYNC_TOKEN).map(ToString::to_string),
        tz: entry.text(CALENDAR_TIMEZONE).map(ToString::to_string),
        supported_reports: entry.supported_reports(),
    })
}

// --- typed, the way the crate's mappers read now ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ItemProps {
    getetag: Option<String>,
    calendar_data: Option<String>,
}

pub fn new_items(xml: &str) -> Out {
    let ms = parse_multistatus_as::<ItemProps>(xml).unwrap();
    let items: Vec<_> = ms
        .responses
        .into_iter()
        .filter_map(|r: WebdavResponseOf<ItemProps>| {
            if r.href.ends_with('/') || r.id().is_empty() {
                return None;
            }
            let id = r.id().to_string();
            let data = r.props.calendar_data.filter(|data| !data.is_empty())?;
            Some(CaldavItemEntry {
                id,
                etag: etag(r.props.getetag),
                data: data.into_bytes(),
            })
        })
        .collect();
    Out::Items(items)
}

#[derive(Debug, Deserialize)]
struct RefProps {
    getetag: Option<String>,
}

pub fn new_refs(xml: &str) -> Out {
    let ms = parse_multistatus_as::<RefProps>(xml).unwrap();
    let mut refs = Vec::new();
    let mut removed = Vec::new();
    for r in ms.responses {
        if r.href.ends_with('/') || r.id().is_empty() {
            continue;
        }
        if r.status == Some(404) {
            removed.push(r.id().to_string());
            continue;
        }
        refs.push(CaldavItemRef {
            id: r.id().to_string(),
            etag: etag(r.props.getetag),
        });
    }
    Out::Refs(refs, removed, ms.sync_token)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CalendarProps {
    resourcetype: Option<BTreeMap<String, ()>>,
    displayname: Option<String>,
    calendar_description: Option<String>,
    calendar_color: Option<String>,
    calendar_timezone: Option<String>,
    getctag: Option<String>,
    sync_token: Option<String>,
    supported_calendar_component_set: Option<CompSet>,
    supported_report_set: Option<ReportSet>,
}

#[derive(Debug, Deserialize)]
struct CompSet {
    #[serde(default)]
    comp: Vec<Comp>,
}

#[derive(Debug, Deserialize)]
struct Comp {
    #[serde(rename = "@name")]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ReportSet {
    #[serde(default)]
    supported_report: Vec<SupportedReport>,
}

#[derive(Debug, Deserialize)]
struct SupportedReport {
    #[serde(default)]
    report: BTreeMap<String, ()>,
}

pub fn new_calendars(xml: &str) -> Out {
    let ms = parse_multistatus_as::<CalendarProps>(xml).unwrap();
    let calendars: Vec<_> = ms
        .responses
        .into_iter()
        .filter_map(|r| {
            let props = r.props;
            if !props.resourcetype?.contains_key("calendar") {
                return None;
            }
            let id = collection_id(HOME, &r.href);
            if id.is_empty() {
                return None;
            }
            Some(CaldavCalendar {
                id,
                display_name: text(props.displayname),
                description: text(props.calendar_description),
                color: text(props.calendar_color),
                components: props
                    .supported_calendar_component_set
                    .map(|set| set.comp.into_iter().filter_map(|comp| comp.name).collect())
                    .unwrap_or_default(),
                ctag: text(props.getctag),
                sync_token: text(props.sync_token),
                tz: text(props.calendar_timezone),
                supported_reports: props
                    .supported_report_set
                    .map(|set| {
                        set.supported_report
                            .into_iter()
                            .flat_map(|report| report.report.into_keys())
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect();
    Out::Calendars(calendars)
}

fn etag(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
        .map(|text| text.trim_matches('"').to_string())
}

fn text(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
}

/// Runs every typed reading on a body, outcomes ignored: the fuzzer only
/// watches for panics here.
pub fn fuzz_typed(xml: &str) {
    let _ = parse_multistatus_as::<ItemProps>(xml);
    let _ = parse_multistatus_as::<RefProps>(xml);
    let _ = parse_multistatus_as::<CalendarProps>(xml);
    let _ = parse_multistatus_as::<BTreeMap<String, serde::de::IgnoredAny>>(xml);
    let _ = parse_multistatus_as::<BTreeMap<&str, &str>>(xml);
}
