//! # REPORT bodies
//!
//! [`parse_report`] reads the body of a `REPORT` request into the report it
//! names: the CalDAV `calendar-multiget`, `calendar-query` and
//! `free-busy-query` (RFC 4791 §7), the CardDAV `addressbook-multiget` and
//! `addressbook-query` (RFC 6352 §8), and `sync-collection` (RFC 6578 §3).
//!
//! A calendar query filter is modelled here whole; an address book query
//! filter reads into the [`CarddavFilter`] the client sends, so a filter
//! round-trips between the two sides.

use core::num::NonZeroU32;

use alloc::{string::String, vec::Vec};

use crate::{
    rfc4791::calendar::CALDAV,
    rfc4918::DAV,
    rfc6352::{
        addressbook::CARDDAV,
        filter::{
            CarddavFilter, CarddavFilterTest, CarddavMatchType, CarddavParamCond,
            CarddavParamFilter, CarddavPropCond, CarddavPropFilter, CarddavTextMatch,
        },
    },
    server::{
        body::{
            WebdavBodyParseError, WebdavDataRequest, WebdavPropSelection, XmlNode, XmlTree,
            parse_tree,
        },
        name::WebdavName,
    },
};

/// A `REPORT` request, by the report its root names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavReportParams {
    /// `C:calendar-multiget` (RFC 4791 §7.9).
    CalendarMultiget(WebdavMultigetParams),
    /// `C:calendar-query` (RFC 4791 §7.8).
    CalendarQuery(CaldavCalendarQueryParams),
    /// `C:free-busy-query` (RFC 4791 §7.10).
    FreeBusyQuery(CaldavTimeRange),
    /// `CR:addressbook-multiget` (RFC 6352 §8.7).
    AddressbookMultiget(WebdavMultigetParams),
    /// `CR:addressbook-query` (RFC 6352 §8.6).
    AddressbookQuery(CarddavAddressbookQueryParams),
    /// `DAV:sync-collection` (RFC 6578 §3.2).
    SyncCollection(WebdavSyncCollectionParams),
    /// Any other report, which a server answers with the
    /// `DAV:supported-report` precondition (RFC 3253 §3.6).
    Other(WebdavName),
}

/// A `calendar-multiget` or `addressbook-multiget` report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavMultigetParams {
    /// The properties asked for each resource.
    pub props: WebdavPropSelection,
    /// The `calendar-data` or `address-data` asked, when it narrows the data.
    pub data: Option<WebdavDataRequest>,
    /// The resource hrefs, as sent.
    pub hrefs: Vec<String>,
}

/// A `calendar-query` report (RFC 4791 §7.8).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaldavCalendarQueryParams {
    /// The properties asked for each match.
    pub props: WebdavPropSelection,
    /// The `calendar-data` asked, when it narrows the data.
    pub data: Option<WebdavDataRequest>,
    /// The top-level `VCALENDAR` comp-filter.
    pub filter: CaldavCompFilter,
    /// The `C:timezone` a floating time is read in (RFC 4791 §9.8).
    pub timezone: Option<String>,
}

/// A `C:comp-filter` (RFC 4791 §9.7.1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaldavCompFilter {
    /// The component name, such as `VEVENT`.
    pub name: String,
    /// What the component must satisfy.
    pub cond: CaldavCompCond,
}

/// What a comp-filter tests (RFC 4791 §9.7.1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaldavCompCond {
    /// The component is absent.
    IsNotDefined,
    /// The component is present and satisfies every test.
    Match {
        /// The range the component must overlap.
        time_range: Option<CaldavTimeRange>,
        /// Tests on its properties.
        props: Vec<CaldavPropFilter>,
        /// Tests on its subcomponents.
        comps: Vec<CaldavCompFilter>,
    },
}

/// A `C:prop-filter` (RFC 4791 §9.7.2).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaldavPropFilter {
    /// The property name, such as `UID`.
    pub name: String,
    /// What the property must satisfy.
    pub cond: CaldavPropCond,
}

/// What a calendar prop-filter tests (RFC 4791 §9.7.2).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaldavPropCond {
    /// The property is absent.
    IsNotDefined,
    /// The property is present and satisfies every test.
    Match {
        /// The range a date-time property must fall in.
        time_range: Option<CaldavTimeRange>,
        /// A match against the property value.
        text: Option<CaldavTextMatch>,
        /// Tests on its parameters.
        params: Vec<CaldavParamFilter>,
    },
}

/// A `C:param-filter` (RFC 4791 §9.7.3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaldavParamFilter {
    /// The parameter name, such as `PARTSTAT`.
    pub name: String,
    /// What the parameter must satisfy, [`None`] when it only has to be
    /// present.
    pub cond: Option<CaldavParamCond>,
}

/// What a calendar param-filter tests (RFC 4791 §9.7.3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaldavParamCond {
    /// The parameter is absent.
    IsNotDefined,
    /// The parameter value matches.
    TextMatch(CaldavTextMatch),
}

/// A `C:text-match` (RFC 4791 §9.7.5): a substring match.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaldavTextMatch {
    /// The text to find.
    pub value: String,
    /// Whether the match is inverted (`negate-condition="yes"`).
    pub negate: bool,
    /// The collation, [`None`] for the default `i;ascii-casemap`.
    pub collation: Option<String>,
}

/// A `C:time-range` or `C:expand` (RFC 4791 §9.9), its bounds as sent:
/// UTC date-times such as `20261118T000000Z`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaldavTimeRange {
    /// The inclusive start, [`None`] for the beginning of time.
    pub start: Option<String>,
    /// The exclusive end, [`None`] for the end of time.
    pub end: Option<String>,
}

/// An `addressbook-query` report (RFC 6352 §8.6).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarddavAddressbookQueryParams {
    /// The properties asked for each match.
    pub props: WebdavPropSelection,
    /// The `address-data` asked, when it narrows the data.
    pub data: Option<WebdavDataRequest>,
    /// The filter.
    pub filter: CarddavFilter,
    /// The `CR:limit` result cap (RFC 6352 §8.6.1).
    pub limit: Option<NonZeroU32>,
}

/// A `sync-collection` report (RFC 6578 §6.1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavSyncCollectionParams {
    /// The token of the last sync, [`None`] for an initial one.
    pub sync_token: Option<String>,
    /// How deep the members are reported.
    pub level: WebdavSyncLevel,
    /// The `DAV:limit` result cap (RFC 5323 §5.17).
    pub limit: Option<NonZeroU32>,
    /// The properties asked for each changed member.
    pub props: WebdavPropSelection,
    /// The `calendar-data` or `address-data` asked, when it narrows the data.
    pub data: Option<WebdavDataRequest>,
}

/// The `DAV:sync-level` of a `sync-collection` (RFC 6578 §6.3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebdavSyncLevel {
    /// The direct members.
    One,
    /// Every member, at any depth.
    Infinite,
}

impl CaldavTimeRange {
    /// Reads a time-range or expand element, refusing one bounded neither
    /// way.
    pub(crate) fn from_node(node: &XmlNode<'_>) -> Result<Self, WebdavBodyParseError> {
        let range = Self {
            start: node.attr("start"),
            end: node.attr("end"),
        };

        if range.start.is_none() && range.end.is_none() {
            return Err(WebdavBodyParseError::Missing("time-range start or end"));
        }

        Ok(range)
    }
}

/// Parses a `REPORT` request body (RFC 3253 §3.6).
pub fn parse_report(body: &[u8]) -> Result<WebdavReportParams, WebdavBodyParseError> {
    let Some(tree) = parse_tree(body)? else {
        return Err(WebdavBodyParseError::Missing("report"));
    };

    let root = &tree.root;

    if root.is_caldav("calendar-multiget") {
        return Ok(WebdavReportParams::CalendarMultiget(multiget(&tree)?));
    }

    if root.is_carddav("addressbook-multiget") {
        return Ok(WebdavReportParams::AddressbookMultiget(multiget(&tree)?));
    }

    if root.is_caldav("calendar-query") {
        let filter = root
            .child(CALDAV.uri, "filter")
            .and_then(|filter| filter.child(CALDAV.uri, "comp-filter"))
            .ok_or(WebdavBodyParseError::Missing("filter"))?;

        return Ok(WebdavReportParams::CalendarQuery(
            CaldavCalendarQueryParams {
                props: selection(&tree)?,
                data: tree.data_request(root)?,
                filter: comp_filter(filter)?,
                timezone: root
                    .child(CALDAV.uri, "timezone")
                    .map(|timezone| timezone.text.clone()),
            },
        ));
    }

    if root.is_caldav("free-busy-query") {
        let range = root
            .child(CALDAV.uri, "time-range")
            .ok_or(WebdavBodyParseError::Missing("time-range"))?;
        return Ok(WebdavReportParams::FreeBusyQuery(
            CaldavTimeRange::from_node(range)?,
        ));
    }

    if root.is_carddav("addressbook-query") {
        let filter = root
            .child(CARDDAV.uri, "filter")
            .ok_or(WebdavBodyParseError::Missing("filter"))?;
        let limit = match root.child(CARDDAV.uri, "limit") {
            Some(limit) => Some(nresults(limit, CARDDAV.uri)?),
            None => None,
        };

        return Ok(WebdavReportParams::AddressbookQuery(
            CarddavAddressbookQueryParams {
                props: selection(&tree)?,
                data: tree.data_request(root)?,
                filter: card_filter(filter)?,
                limit,
            },
        ));
    }

    if root.is_dav("sync-collection") {
        let token = root
            .dav("sync-token")
            .ok_or(WebdavBodyParseError::Missing("sync-token"))?;
        let level = match root.dav("sync-level").map(XmlNode::trimmed) {
            Some("1") => WebdavSyncLevel::One,
            Some("infinite") => WebdavSyncLevel::Infinite,
            Some(level) => {
                return Err(WebdavBodyParseError::Invalid {
                    element: "sync-level",
                    value: level.into(),
                });
            }
            None => return Err(WebdavBodyParseError::Missing("sync-level")),
        };
        let limit = match root.dav("limit") {
            Some(limit) => Some(nresults(limit, DAV.uri)?),
            None => None,
        };

        return Ok(WebdavReportParams::SyncCollection(
            WebdavSyncCollectionParams {
                sync_token: Some(token.trimmed())
                    .filter(|token| !token.is_empty())
                    .map(String::from),
                level,
                limit,
                props: selection(&tree)?,
                data: tree.data_request(root)?,
            },
        ));
    }

    Ok(WebdavReportParams::Other(root.name()))
}

/// The property selection of the report root, required.
fn selection(tree: &XmlTree<'_>) -> Result<WebdavPropSelection, WebdavBodyParseError> {
    tree.selection(&tree.root)
        .ok_or(WebdavBodyParseError::Missing("prop"))
}

/// Reads a multiget root.
fn multiget(tree: &XmlTree<'_>) -> Result<WebdavMultigetParams, WebdavBodyParseError> {
    let hrefs: Vec<String> = tree
        .root
        .children
        .iter()
        .filter(|child| child.is_dav("href"))
        .map(|href| href.trimmed().into())
        .collect();

    if hrefs.is_empty() {
        return Err(WebdavBodyParseError::Missing("href"));
    }

    Ok(WebdavMultigetParams {
        props: selection(tree)?,
        data: tree.data_request(&tree.root)?,
        hrefs,
    })
}

/// Reads the `nresults` of a `limit` element in namespace `ns`.
fn nresults(limit: &XmlNode<'_>, ns: &str) -> Result<NonZeroU32, WebdavBodyParseError> {
    let text = limit
        .child(ns, "nresults")
        .ok_or(WebdavBodyParseError::Missing("nresults"))?
        .trimmed();

    text.parse().map_err(|_| WebdavBodyParseError::Invalid {
        element: "nresults",
        value: text.into(),
    })
}

/// The `name` attribute of a filter element, required.
fn name_of(node: &XmlNode<'_>) -> Result<String, WebdavBodyParseError> {
    node.attr("name")
        .ok_or(WebdavBodyParseError::Missing("name"))
}

/// Whether a `negate-condition` attribute says yes.
fn negated(node: &XmlNode<'_>) -> Result<bool, WebdavBodyParseError> {
    match node.attr("negate-condition").as_deref() {
        None | Some("no") => Ok(false),
        Some("yes") => Ok(true),
        Some(value) => Err(WebdavBodyParseError::Invalid {
            element: "negate-condition",
            value: value.into(),
        }),
    }
}

/// Reads a calendar comp-filter.
fn comp_filter(node: &XmlNode<'_>) -> Result<CaldavCompFilter, WebdavBodyParseError> {
    let name = name_of(node)?;

    if node.child(CALDAV.uri, "is-not-defined").is_some() {
        let cond = CaldavCompCond::IsNotDefined;
        return Ok(CaldavCompFilter { name, cond });
    }

    let mut time_range = None;
    let mut props = Vec::new();
    let mut comps = Vec::new();

    for child in &node.children {
        if child.is_caldav("time-range") {
            time_range = Some(CaldavTimeRange::from_node(child)?);
        } else if child.is_caldav("prop-filter") {
            props.push(cal_prop_filter(child)?);
        } else if child.is_caldav("comp-filter") {
            comps.push(comp_filter(child)?);
        }
    }

    let cond = CaldavCompCond::Match {
        time_range,
        props,
        comps,
    };
    Ok(CaldavCompFilter { name, cond })
}

/// Reads a calendar prop-filter.
fn cal_prop_filter(node: &XmlNode<'_>) -> Result<CaldavPropFilter, WebdavBodyParseError> {
    let name = name_of(node)?;

    if node.child(CALDAV.uri, "is-not-defined").is_some() {
        let cond = CaldavPropCond::IsNotDefined;
        return Ok(CaldavPropFilter { name, cond });
    }

    let mut time_range = None;
    let mut text = None;
    let mut params = Vec::new();

    for child in &node.children {
        if child.is_caldav("time-range") {
            time_range = Some(CaldavTimeRange::from_node(child)?);
        } else if child.is_caldav("text-match") {
            text = Some(cal_text_match(child)?);
        } else if child.is_caldav("param-filter") {
            let name = name_of(child)?;
            let cond = if child.child(CALDAV.uri, "is-not-defined").is_some() {
                Some(CaldavParamCond::IsNotDefined)
            } else {
                match child.child(CALDAV.uri, "text-match") {
                    Some(text) => Some(CaldavParamCond::TextMatch(cal_text_match(text)?)),
                    None => None,
                }
            };
            params.push(CaldavParamFilter { name, cond });
        }
    }

    let cond = CaldavPropCond::Match {
        time_range,
        text,
        params,
    };
    Ok(CaldavPropFilter { name, cond })
}

/// Reads a calendar text-match.
fn cal_text_match(node: &XmlNode<'_>) -> Result<CaldavTextMatch, WebdavBodyParseError> {
    Ok(CaldavTextMatch {
        value: node.text.clone(),
        negate: negated(node)?,
        collation: node.attr("collation"),
    })
}

/// Reads the `test` attribute of an address book filter, `anyof` by default.
fn filter_test(node: &XmlNode<'_>) -> Result<CarddavFilterTest, WebdavBodyParseError> {
    match node.attr("test").as_deref() {
        None | Some("anyof") => Ok(CarddavFilterTest::AnyOf),
        Some("allof") => Ok(CarddavFilterTest::AllOf),
        Some(value) => Err(WebdavBodyParseError::Invalid {
            element: "test",
            value: value.into(),
        }),
    }
}

/// Reads an address book filter.
fn card_filter(node: &XmlNode<'_>) -> Result<CarddavFilter, WebdavBodyParseError> {
    let mut props = Vec::new();

    for child in &node.children {
        if !child.is_carddav("prop-filter") {
            continue;
        }

        let name = name_of(child)?;
        let test = filter_test(child)?;

        if child.child(CARDDAV.uri, "is-not-defined").is_some() {
            let cond = CarddavPropCond::IsNotDefined;
            props.push(CarddavPropFilter { name, test, cond });
            continue;
        }

        let mut texts = Vec::new();
        let mut params = Vec::new();

        for grandchild in &child.children {
            if grandchild.is_carddav("text-match") {
                texts.push(card_text_match(grandchild)?);
            } else if grandchild.is_carddav("param-filter") {
                let name = name_of(grandchild)?;
                let cond = if grandchild.child(CARDDAV.uri, "is-not-defined").is_some() {
                    Some(CarddavParamCond::IsNotDefined)
                } else {
                    match grandchild.child(CARDDAV.uri, "text-match") {
                        Some(text) => Some(CarddavParamCond::TextMatch(card_text_match(text)?)),
                        None => None,
                    }
                };
                params.push(CarddavParamFilter { name, cond });
            }
        }

        let cond = CarddavPropCond::Match { texts, params };
        props.push(CarddavPropFilter { name, test, cond });
    }

    Ok(CarddavFilter {
        test: filter_test(node)?,
        props,
    })
}

/// Reads an address book text-match.
fn card_text_match(node: &XmlNode<'_>) -> Result<CarddavTextMatch, WebdavBodyParseError> {
    let match_type = match node.attr("match-type").as_deref() {
        None | Some("contains") => CarddavMatchType::Contains,
        Some("equals") => CarddavMatchType::Equals,
        Some("starts-with") => CarddavMatchType::StartsWith,
        Some("ends-with") => CarddavMatchType::EndsWith,
        Some(value) => {
            return Err(WebdavBodyParseError::Invalid {
                element: "match-type",
                value: value.into(),
            });
        }
    };

    Ok(CarddavTextMatch {
        value: node.text.clone(),
        match_type,
        negate: negated(node)?,
        collation: node.attr("collation"),
    })
}
