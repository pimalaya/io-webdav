//! Offline coverage of the server side, the client side as its oracle: each
//! request body the client builds parses back to what the client meant, and
//! each body the server writes reads back, through the client coroutines, to
//! what the server meant. Plus the request bodies of the RFC examples.

#![cfg(all(feature = "client", feature = "server"))]

mod common;

use std::{collections::BTreeSet, num::NonZeroU32};

use common::*;
use io_webdav::{
    rfc4791::{
        calendar::{
            CALDAV, CALENDAR, CALENDAR_COLOR, CALENDAR_DESCRIPTION, CALENDAR_MULTIGET,
            CALENDAR_QUERY, CaldavCalendar, calendar_multiget_body, list::CaldavCalendarList,
            mkcalendar_body, property_set,
        },
        item::{create::CaldavItemCreate, list::CaldavItemList},
    },
    rfc4918::{
        DAV, DISPLAYNAME, GETCTAG, GETETAG, RESOURCETYPE, WebdavAuth, WebdavPropValue,
        WebdavProperty, mkcol_body, parse_multistatus, propfind_body, proppatch_body,
        send::WebdavSendError,
    },
    rfc6352::{
        addressbook::{ADDRESSBOOK, ADDRESSBOOK_DESCRIPTION, addressbook_multiget_body},
        card::list::{CarddavCardList, CarddavCardListOptions},
        filter::{
            CarddavFilter, CarddavFilterTest, CarddavMatchType, CarddavParamCond,
            CarddavParamFilter, CarddavPropCond, CarddavPropFilter, CarddavTextMatch,
        },
    },
    rfc6578::sync_collection::{
        WebdavSyncCollection, WebdavSyncCollectionError, sync_collection_body,
    },
    server::{
        body::{WebdavBodyParseError, WebdavPropSelection},
        header::{WebdavDepth, WebdavTimeout},
        lock::{WebdavActiveLock, WebdavLockInfo, WebdavLockScope, parse_lockinfo},
        mkcol::{parse_mkcalendar, parse_mkcol},
        multistatus::{
            WebdavDataType, WebdavMultistatusBody, WebdavPropBody, WebdavPropContent,
            WebdavResponseBody,
        },
        name::{WebdavName, WebdavPropName},
        precondition::WebdavPrecondition,
        propfind::parse_propfind,
        proppatch::{WebdavPropUpdate, parse_proppatch},
        report::{
            CaldavCompCond, CaldavCompFilter, CaldavParamCond, CaldavParamFilter, CaldavPropCond,
            CaldavPropFilter, CaldavTextMatch, CaldavTimeRange, WebdavReportParams,
            WebdavSyncLevel, parse_report,
        },
    },
};
use url::Url;

const UA: &str = "io-webdav/test";

const CALENDAR_DATA: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar-data",
};

fn base() -> Url {
    Url::parse("https://dav.example.org/").unwrap()
}

/// The body of an HTTP request a coroutine wrote.
fn request_body(request: &[u8]) -> Vec<u8> {
    let at = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    request[at + 4..].to_vec()
}

fn prop(name: WebdavPropName, content: WebdavPropContent) -> WebdavPropBody {
    WebdavPropBody { name, content }
}

fn text(value: &str) -> WebdavPropContent {
    WebdavPropContent::Text(value.into())
}

fn dav(local: &'static str) -> WebdavProperty {
    WebdavProperty { ns: DAV, local }
}

#[test]
fn a_client_propfind_parses_back_to_its_properties() {
    let body = propfind_body(&[DISPLAYNAME, GETCTAG, RESOURCETYPE, CALENDAR_COLOR]);
    assert_eq!(
        parse_propfind(&body).unwrap(),
        WebdavPropSelection::Prop(vec![
            WebdavPropName::DisplayName,
            WebdavPropName::Getctag,
            WebdavPropName::ResourceType,
            WebdavPropName::CalendarColor,
        ])
    );
}

#[test]
fn a_client_proppatch_parses_back_to_its_instructions() {
    let set = [
        (DISPLAYNAME, WebdavPropValue::Text("A & <B>")),
        (CALENDAR_COLOR, WebdavPropValue::Text("#ff0000")),
    ];
    let body = proppatch_body(&set, &[CALENDAR_DESCRIPTION]);
    let updates = parse_proppatch(&body).unwrap();

    assert_eq!(updates.len(), 3);
    let WebdavPropUpdate::Set(name) = &updates[0] else {
        panic!("not a set: {:?}", updates[0]);
    };
    assert_eq!(name.name, WebdavPropName::DisplayName);
    assert_eq!(name.text, "A & <B>");
    let WebdavPropUpdate::Set(color) = &updates[1] else {
        panic!("not a set: {:?}", updates[1]);
    };
    assert_eq!(
        (color.name.clone(), color.text.as_str()),
        (WebdavPropName::CalendarColor, "#ff0000")
    );
    assert_eq!(
        updates[2],
        WebdavPropUpdate::Remove(WebdavPropName::CalendarDescription)
    );
}

#[test]
fn a_client_mkcol_parses_back_to_its_resource_types_and_values() {
    let set = [
        (DISPLAYNAME, WebdavPropValue::Text("Contacts")),
        (ADDRESSBOOK_DESCRIPTION, WebdavPropValue::Text("Mine")),
    ];
    let props = parse_mkcol(&mkcol_body(&[ADDRESSBOOK], &set)).unwrap();

    assert_eq!(props.len(), 3);
    assert_eq!(props[0].name, WebdavPropName::ResourceType);
    let types: Vec<_> = props[0].children.iter().map(|c| c.name.clone()).collect();
    assert_eq!(types, [dav("collection").into(), ADDRESSBOOK.into()]);
    assert_eq!(props[1].text, "Contacts");
    assert_eq!(props[2].name, WebdavPropName::AddressbookDescription);
    assert_eq!(props[2].text, "Mine");
}

#[test]
fn a_client_mkcalendar_parses_back_to_the_calendar() {
    let calendar = CaldavCalendar {
        id: "work".into(),
        display_name: Some("Work".into()),
        color: Some("#00ff00".into()),
        description: Some("Office".into()),
        components: ["VEVENT".to_string(), "VTODO".to_string()].into(),
        tz: Some("BEGIN:VTIMEZONE\r\nEND:VTIMEZONE".into()),
        ..Default::default()
    };
    let props = parse_mkcalendar(&mkcalendar_body(&property_set(&calendar))).unwrap();

    let text = |name: WebdavPropName| {
        props
            .iter()
            .find(|prop| prop.name == name)
            .map(|prop| prop.text.clone())
    };
    assert_eq!(text(WebdavPropName::DisplayName).as_deref(), Some("Work"));
    assert_eq!(
        text(WebdavPropName::CalendarColor).as_deref(),
        Some("#00ff00")
    );
    assert_eq!(
        text(WebdavPropName::CalendarDescription).as_deref(),
        Some("Office")
    );
    assert_eq!(
        text(WebdavPropName::CalendarTimezone).as_deref(),
        Some("BEGIN:VTIMEZONE\nEND:VTIMEZONE")
    );

    let comps = props
        .iter()
        .find(|prop| prop.name == WebdavPropName::SupportedCalendarComponentSet)
        .unwrap();
    let names: BTreeSet<_> = comps
        .children
        .iter()
        .filter_map(|child| child.name_attr.clone())
        .collect();
    assert_eq!(names, calendar.components);
}

#[test]
fn client_multigets_parse_back_to_their_hrefs() {
    let hrefs = vec!["/cal/a.ics".to_string(), "/cal/b%20c.ics".to_string()];
    let body = calendar_multiget_body(&hrefs, &[GETETAG, CALENDAR_DATA]);
    let WebdavReportParams::CalendarMultiget(multiget) = parse_report(&body).unwrap() else {
        panic!("not a calendar-multiget");
    };
    assert_eq!(multiget.hrefs, hrefs);
    assert_eq!(
        multiget.props,
        WebdavPropSelection::Prop(vec![WebdavPropName::GetEtag, WebdavPropName::CalendarData])
    );
    assert_eq!(multiget.data, None);

    let body = addressbook_multiget_body(&hrefs, &[GETETAG]);
    let WebdavReportParams::AddressbookMultiget(multiget) = parse_report(&body).unwrap() else {
        panic!("not an addressbook-multiget");
    };
    assert_eq!(multiget.hrefs, hrefs);
}

#[test]
fn a_client_card_filter_parses_back_whole() {
    let filter = CarddavFilter {
        test: CarddavFilterTest::AnyOf,
        props: vec![
            CarddavPropFilter {
                name: "EMAIL".into(),
                test: CarddavFilterTest::AllOf,
                cond: CarddavPropCond::Match {
                    texts: vec![CarddavTextMatch {
                        value: "@example.org".into(),
                        match_type: CarddavMatchType::EndsWith,
                        negate: true,
                        collation: Some("i;octet".into()),
                    }],
                    params: vec![
                        CarddavParamFilter {
                            name: "TYPE".into(),
                            cond: Some(CarddavParamCond::TextMatch(CarddavTextMatch {
                                value: "work".into(),
                                match_type: CarddavMatchType::Equals,
                                ..Default::default()
                            })),
                        },
                        CarddavParamFilter {
                            name: "PREF".into(),
                            cond: Some(CarddavParamCond::IsNotDefined),
                        },
                        CarddavParamFilter {
                            name: "LABEL".into(),
                            cond: None,
                        },
                    ],
                },
            },
            CarddavPropFilter {
                name: "NICKNAME".into(),
                test: CarddavFilterTest::AnyOf,
                cond: CarddavPropCond::IsNotDefined,
            },
        ],
    };
    let opts = CarddavCardListOptions {
        filter: filter.clone(),
        limit: NonZeroU32::new(25),
    };

    let mut list = CarddavCardList::new(&base(), &WebdavAuth::None, UA, "/book/", &opts);
    let body = request_body(&expect_wants_write(&mut list, None));
    let WebdavReportParams::AddressbookQuery(query) = parse_report(&body).unwrap() else {
        panic!("not an addressbook-query");
    };
    assert_eq!(query.filter, filter);
    assert_eq!(query.limit, NonZeroU32::new(25));
}

#[test]
fn a_client_calendar_query_parses_back_to_its_filter() {
    let comp_filter = "<C:comp-filter name=\"VEVENT\">\
        <C:time-range start=\"20261101T000000Z\" end=\"20261201T000000Z\"/>\
        <C:prop-filter name=\"SUMMARY\"><C:text-match negate-condition=\"yes\">\
        party &amp; co</C:text-match></C:prop-filter>\
        <C:prop-filter name=\"ATTENDEE\"><C:param-filter name=\"PARTSTAT\">\
        <C:text-match collation=\"i;octet\">NEEDS-ACTION</C:text-match>\
        </C:param-filter></C:prop-filter>\
        <C:comp-filter name=\"VALARM\"><C:is-not-defined/></C:comp-filter>\
        </C:comp-filter>";
    let mut list = CaldavItemList::new(&base(), &WebdavAuth::None, UA, "/cal/", comp_filter);
    let body = request_body(&expect_wants_write(&mut list, None));
    let WebdavReportParams::CalendarQuery(query) = parse_report(&body).unwrap() else {
        panic!("not a calendar-query");
    };

    let event = CaldavCompFilter {
        name: "VEVENT".into(),
        cond: CaldavCompCond::Match {
            time_range: Some(CaldavTimeRange {
                start: Some("20261101T000000Z".into()),
                end: Some("20261201T000000Z".into()),
            }),
            props: vec![
                CaldavPropFilter {
                    name: "SUMMARY".into(),
                    cond: CaldavPropCond::Match {
                        time_range: None,
                        text: Some(CaldavTextMatch {
                            value: "party & co".into(),
                            negate: true,
                            collation: None,
                        }),
                        params: vec![],
                    },
                },
                CaldavPropFilter {
                    name: "ATTENDEE".into(),
                    cond: CaldavPropCond::Match {
                        time_range: None,
                        text: None,
                        params: vec![CaldavParamFilter {
                            name: "PARTSTAT".into(),
                            cond: Some(CaldavParamCond::TextMatch(CaldavTextMatch {
                                value: "NEEDS-ACTION".into(),
                                negate: false,
                                collation: Some("i;octet".into()),
                            })),
                        }],
                    },
                },
            ],
            comps: vec![CaldavCompFilter {
                name: "VALARM".into(),
                cond: CaldavCompCond::IsNotDefined,
            }],
        },
    };
    let want = CaldavCompFilter {
        name: "VCALENDAR".into(),
        cond: CaldavCompCond::Match {
            time_range: None,
            props: vec![],
            comps: vec![event],
        },
    };
    assert_eq!(query.filter, want);
    assert_eq!(query.timezone, None);
}

#[test]
fn a_client_sync_collection_parses_back_to_its_token() {
    let body = sync_collection_body(Some("http://example.com/ns/sync/12"), &[GETETAG]);
    let WebdavReportParams::SyncCollection(sync) = parse_report(&body).unwrap() else {
        panic!("not a sync-collection");
    };
    assert_eq!(
        sync.sync_token.as_deref(),
        Some("http://example.com/ns/sync/12")
    );
    assert_eq!(sync.level, WebdavSyncLevel::One);
    assert_eq!(
        sync.props,
        WebdavPropSelection::Prop(vec![WebdavPropName::GetEtag])
    );

    let body = sync_collection_body(None, &[GETETAG]);
    let WebdavReportParams::SyncCollection(sync) = parse_report(&body).unwrap() else {
        panic!("not a sync-collection");
    };
    assert_eq!(sync.sync_token, None);
}

/// Answers a PROPFIND for a home set holding one calendar.
fn calendar_listing(selection: &WebdavPropSelection) -> WebdavMultistatusBody {
    let WebdavPropSelection::Prop(names) = selection else {
        panic!("not a prop selection: {selection:?}");
    };

    let mut home = WebdavResponseBody {
        href: "/dav/calendars/".into(),
        ..Default::default()
    };
    let mut calendar = WebdavResponseBody {
        href: "/dav/calendars/R%C3%A9union%20&%20co/".into(),
        ..Default::default()
    };

    for name in names {
        let content = match name {
            WebdavPropName::ResourceType => {
                let types = WebdavPropContent::ResourceType(vec![dav("collection")]);
                home.push(200, prop(name.clone(), types));
                WebdavPropContent::ResourceType(vec![dav("collection"), CALENDAR])
            }
            WebdavPropName::DisplayName => text("Réunion & <co>"),
            WebdavPropName::CalendarColor => text("#336699"),
            WebdavPropName::SupportedCalendarComponentSet => {
                WebdavPropContent::Components(vec!["VEVENT".into(), "VTODO".into()])
            }
            WebdavPropName::Getctag => text("ctag-7"),
            WebdavPropName::SyncToken => text("http://example.com/ns/sync/7"),
            WebdavPropName::SupportedReportSet => WebdavPropContent::Reports(vec![
                CALENDAR_QUERY,
                CALENDAR_MULTIGET,
                dav("sync-collection"),
            ]),
            _ => {
                calendar.push(404, prop(name.clone(), WebdavPropContent::Empty));
                continue;
            }
        };
        calendar.push(200, prop(name.clone(), content));
    }

    WebdavMultistatusBody {
        responses: vec![home, calendar],
        ..Default::default()
    }
}

#[test]
fn a_calendar_listing_round_trips_through_the_client() {
    let mut list = CaldavCalendarList::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/");
    let request = expect_wants_write(&mut list, None);
    let selection = parse_propfind(&request_body(&request)).unwrap();

    let reply = calendar_listing(&selection).to_xml();
    expect_wants_read(&mut list);
    let calendars = expect_complete(&mut list, &multistatus_response(&reply)).unwrap();

    let calendar = calendars.into_iter().next().unwrap();
    assert_eq!(calendar.id, "Réunion & co");
    assert_eq!(calendar.href, "/dav/calendars/R%C3%A9union%20&%20co/");
    assert_eq!(calendar.display_name.as_deref(), Some("Réunion & <co>"));
    assert_eq!(calendar.color.as_deref(), Some("#336699"));
    assert_eq!(calendar.description, None);
    assert_eq!(calendar.ctag.as_deref(), Some("ctag-7"));
    assert_eq!(
        calendar.sync_token.as_deref(),
        Some("http://example.com/ns/sync/7")
    );
    assert_eq!(
        calendar.components,
        ["VEVENT".to_string(), "VTODO".to_string()].into()
    );
    assert_eq!(
        calendar.supported_reports,
        [
            "calendar-multiget".to_string(),
            "calendar-query".to_string(),
            "sync-collection".to_string(),
        ]
        .into()
    );
}

#[test]
fn every_property_value_reads_back_through_the_scanner() {
    let dead = parse_proppatch(
        br#"<D:propertyupdate xmlns:D="DAV:" xmlns:Z="http://ns.example.com/z/">
          <D:set><D:prop><Z:Authors><Z:Author>Jim &amp; Roy</Z:Author></Z:Authors></D:prop></D:set>
        </D:propertyupdate>"#,
    )
    .unwrap();
    let WebdavPropUpdate::Set(dead) = &dead[0] else {
        panic!("not a set");
    };

    let lock = WebdavActiveLock {
        scope: WebdavLockScope::Exclusive,
        depth: WebdavDepth::Infinity,
        owner: Some("<D:owner xmlns:D=\"DAV:\"><D:href>mailto:a@b</D:href></D:owner>".into()),
        timeout: Some(WebdavTimeout::Seconds(3600)),
        token: "urn:uuid:e71d4fae-5dec-22d6-fea5-00a0c91e6be4".into(),
        root: "/locked".into(),
    };
    let other = WebdavPropName::Other(WebdavName {
        ns: "urn:other".into(),
        local: "flag".into(),
    });

    let mut response = WebdavResponseBody {
        href: "/r".into(),
        ..Default::default()
    };
    let props = [
        (
            WebdavPropName::CurrentUserPrincipal,
            WebdavPropContent::Href("/p/a/".into()),
        ),
        (
            WebdavPropName::CalendarHomeSet,
            WebdavPropContent::Hrefs(vec!["/c/1/".into(), "/c/2/".into()]),
        ),
        (
            WebdavPropName::CurrentUserPrivilegeSet,
            WebdavPropContent::Privileges(vec![dav("read"), dav("write")]),
        ),
        (
            WebdavPropName::SupportedAddressData,
            WebdavPropContent::AddressDataTypes(vec![WebdavDataType {
                content_type: "text/vcard".into(),
                version: "4.0".into(),
            }]),
        ),
        (
            WebdavPropName::SupportedCalendarData,
            WebdavPropContent::CalendarDataTypes(vec![WebdavDataType {
                content_type: "text/calendar".into(),
                version: "2.0".into(),
            }]),
        ),
        (
            WebdavPropName::LockDiscovery,
            WebdavPropContent::Locks(vec![lock]),
        ),
        (
            WebdavPropName::SupportedLock,
            WebdavPropContent::LockScopes(vec![WebdavLockScope::Exclusive]),
        ),
        (WebdavPropName::QuotaUsedBytes, text("1024")),
        (
            dead.name.clone(),
            WebdavPropContent::Element(dead.xml.clone()),
        ),
        (other, text("on")),
        (
            WebdavPropName::CalendarTimezone,
            WebdavPropContent::Xml("<![CDATA[BEGIN:VTIMEZONE]]>".into()),
        ),
    ];
    for (name, content) in props {
        response.push(200, prop(name, content));
    }
    response.push(403, prop(WebdavPropName::GetEtag, WebdavPropContent::Empty));

    let gone = WebdavResponseBody {
        href: "/r/gone.ics".into(),
        status: Some(404),
        ..Default::default()
    };
    let body = WebdavMultistatusBody {
        responses: vec![response, gone],
        description: Some("done".into()),
        sync_token: Some("tok&1".into()),
    };

    let ms = parse_multistatus(&body.to_xml()).unwrap();
    assert_eq!(ms.sync_token.as_deref(), Some("tok&1"));
    assert_eq!(ms.responses[1].status, Some(404));

    let entry = &ms.responses[0];
    let item = |local: &str| entry.props.iter().find(|p| p.local == local).unwrap();
    let child_locals = |local: &str| -> Vec<String> {
        item(local)
            .children
            .iter()
            .map(|c| c.local.clone())
            .collect()
    };

    assert_eq!(item("current-user-principal").text, "/p/a/");
    assert_eq!(child_locals("calendar-home-set"), ["href", "href"]);
    let privileges: Vec<_> = item("current-user-privilege-set")
        .children
        .iter()
        .map(|privilege| privilege.children[0].local.clone())
        .collect();
    assert_eq!(privileges, ["read", "write"]);
    assert_eq!(
        child_locals("supported-address-data"),
        ["address-data-type"]
    );
    assert_eq!(child_locals("supported-calendar-data"), ["calendar-data"]);
    assert_eq!(child_locals("lockdiscovery"), ["activelock"]);
    assert!(item("lockdiscovery").text.contains("urn:uuid:e71d4fae"));
    assert!(item("lockdiscovery").text.contains("Second-3600"));
    assert_eq!(child_locals("supportedlock"), ["lockentry"]);
    assert_eq!(item("quota-used-bytes").text, "1024");
    assert_eq!(item("Authors").text, "Jim & Roy");
    assert_eq!(item("flag").text, "on");
    assert_eq!(item("calendar-timezone").text, "BEGIN:VTIMEZONE");
    assert_eq!(entry.failures.len(), 1);
    assert_eq!(
        (
            entry.failures[0].property.as_str(),
            entry.failures[0].status
        ),
        ("getetag", 403)
    );
}

#[test]
fn a_calendar_query_answer_round_trips_through_the_client() {
    let mut list = CaldavItemList::new(&base(), &WebdavAuth::None, UA, "/cal/", "");
    let request = expect_wants_write(&mut list, None);
    let WebdavReportParams::CalendarQuery(query) = parse_report(&request_body(&request)).unwrap()
    else {
        panic!("not a calendar-query");
    };
    assert_eq!(
        query.props,
        WebdavPropSelection::Prop(vec![WebdavPropName::GetEtag, WebdavPropName::CalendarData])
    );

    let ical = "BEGIN:VCALENDAR\r\nSUMMARY:a < b & c\r\nEND:VCALENDAR\r\n";
    let mut item = WebdavResponseBody {
        href: "/cal/event%201.ics".into(),
        ..Default::default()
    };
    item.push(200, prop(WebdavPropName::GetEtag, text("\"e1\"")));
    item.push(200, prop(WebdavPropName::CalendarData, text(ical)));
    let reply = WebdavMultistatusBody {
        responses: vec![item],
        ..Default::default()
    };

    expect_wants_read(&mut list);
    let ok = expect_complete(&mut list, &multistatus_response(&reply.to_xml())).unwrap();
    let entry = ok.items.into_iter().next().unwrap();
    assert_eq!(entry.id, "event 1.ics");
    assert_eq!(entry.etag.as_deref(), Some("e1"));
    assert_eq!(entry.data, ical.as_bytes());
}

#[test]
fn a_sync_answer_round_trips_through_the_client() {
    let mut sync = WebdavSyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/book/",
        Some("tok-1"),
        &[GETETAG],
        Default::default(),
    );
    let request = expect_wants_write(&mut sync, None);
    let WebdavReportParams::SyncCollection(params) = parse_report(&request_body(&request)).unwrap()
    else {
        panic!("not a sync-collection");
    };
    assert_eq!(params.sync_token.as_deref(), Some("tok-1"));

    let mut changed = WebdavResponseBody {
        href: "/book/a.vcf".into(),
        ..Default::default()
    };
    changed.push(200, prop(WebdavPropName::GetEtag, text("\"e2\"")));
    let removed = WebdavResponseBody {
        href: "/book/b.vcf".into(),
        status: Some(404),
        ..Default::default()
    };
    let reply = WebdavMultistatusBody {
        responses: vec![changed, removed],
        sync_token: Some("tok-2".into()),
        ..Default::default()
    };

    expect_wants_read(&mut sync);
    let delta = expect_complete(&mut sync, &multistatus_response(&reply.to_xml())).unwrap();
    assert_eq!(delta.changed.len(), 1);
    assert_eq!(delta.changed[0].href, "/book/a.vcf");
    assert_eq!(delta.changed[0].etag.as_deref(), Some("e2"));
    assert_eq!(delta.vanished, ["/book/b.vcf"]);
    assert_eq!(delta.sync_token.as_deref(), Some("tok-2"));
    assert!(!delta.truncated);
}

#[test]
fn error_bodies_read_back_as_the_client_preconditions() {
    let mut sync = WebdavSyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/book/",
        Some("stale"),
        &[GETETAG],
        Default::default(),
    );
    let body = WebdavPrecondition::ValidSyncToken.error_body();
    let (_, ret) = expect_exchange(&mut sync, &http_response("403 Forbidden", &[], &body));
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSyncCollectionError::InvalidSyncToken
    ));

    let mut create = CaldavItemCreate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/cal/",
        "event.ics",
        b"BEGIN:VCALENDAR".to_vec(),
    );
    let body = WebdavPrecondition::CalendarNoUidConflict("/cal/other.ics".into()).error_body();
    assert!(body.contains("<C:no-uid-conflict><D:href>/cal/other.ics</D:href>"));
    let (_, ret) = expect_exchange(&mut create, &http_response("409 Conflict", &[], &body));
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSendError::DuplicateUid { status: 409, .. }
    ));
}

#[test]
fn every_precondition_writes_a_well_formed_error() {
    let hrefs = vec!["/a".to_string(), "/b".to_string()];
    let all = [
        WebdavPrecondition::LockTokenMatchesRequestUri,
        WebdavPrecondition::LockTokenSubmitted(hrefs.clone()),
        WebdavPrecondition::NoConflictingLock(vec![]),
        WebdavPrecondition::NoExternalEntities,
        WebdavPrecondition::PreservedLiveProperties,
        WebdavPrecondition::PropfindFiniteDepth,
        WebdavPrecondition::CannotModifyProtectedProperty,
        WebdavPrecondition::ResourceMustBeNull,
        WebdavPrecondition::ValidResourcetype,
        WebdavPrecondition::SupportedReport,
        WebdavPrecondition::NumberOfMatchesWithinLimits,
        WebdavPrecondition::ValidSyncToken,
        WebdavPrecondition::CalendarSupportedData,
        WebdavPrecondition::CalendarValidData,
        WebdavPrecondition::CalendarValidObjectResource,
        WebdavPrecondition::CalendarSupportedComponent,
        WebdavPrecondition::CalendarNoUidConflict("/c".into()),
        WebdavPrecondition::CalendarCollectionLocationOk,
        WebdavPrecondition::CalendarMaxResourceSize,
        WebdavPrecondition::CalendarMinDateTime,
        WebdavPrecondition::CalendarMaxDateTime,
        WebdavPrecondition::CalendarMaxInstances,
        WebdavPrecondition::CalendarMaxAttendeesPerInstance,
        WebdavPrecondition::CalendarValidFilter,
        WebdavPrecondition::CalendarSupportedFilter,
        WebdavPrecondition::CalendarSupportedCollation,
        WebdavPrecondition::AddressbookSupportedData,
        WebdavPrecondition::AddressbookValidData,
        WebdavPrecondition::AddressbookNoUidConflict("/v".into()),
        WebdavPrecondition::AddressbookCollectionLocationOk,
        WebdavPrecondition::AddressbookMaxResourceSize,
        WebdavPrecondition::AddressbookSupportedDataConversion,
        WebdavPrecondition::AddressbookSupportedFilter,
        WebdavPrecondition::AddressbookSupportedCollation,
    ];

    for condition in all {
        let response = WebdavResponseBody {
            href: "/x".into(),
            status: Some(403),
            error: Some(condition.clone()),
            description: Some("refused".into()),
            ..Default::default()
        };
        let body = WebdavMultistatusBody {
            responses: vec![response],
            ..Default::default()
        };
        let ms = parse_multistatus(&body.to_xml()).unwrap();
        assert_eq!(ms.responses[0].status, Some(403), "{condition:?}");

        let error = condition.error_body();
        let wrapped = error
            .replace("<D:error", "<D:multistatus")
            .replace("</D:error>", "</D:multistatus>");
        assert!(parse_multistatus(&wrapped).is_ok(), "{error}");
    }

    let submitted = WebdavPrecondition::LockTokenSubmitted(hrefs).to_xml();
    assert_eq!(
        submitted,
        "<D:lock-token-submitted><D:href>/a</D:href><D:href>/b</D:href></D:lock-token-submitted>"
    );
}

#[test]
fn propfind_reads_the_rfc_4918_examples() {
    let named = br#"<?xml version="1.0" encoding="utf-8" ?>
     <propfind xmlns="DAV:">
       <prop xmlns:R="http://ns.example.com/boxschema/">
         <R:bigbox/>
         <R:author/>
         <R:DingALing/>
         <R:Random/>
       </prop>
     </propfind>"#;
    let WebdavPropSelection::Prop(names) = parse_propfind(named).unwrap() else {
        panic!("not a prop selection");
    };
    let locals: Vec<_> = names.iter().map(|n| n.local()).collect();
    assert_eq!(locals, ["bigbox", "author", "DingALing", "Random"]);
    assert!(
        names
            .iter()
            .all(|n| n.ns() == "http://ns.example.com/boxschema/")
    );

    let propname = br#"<?xml version="1.0" encoding="utf-8" ?>
     <propfind xmlns="DAV:"><propname/></propfind>"#;
    assert_eq!(
        parse_propfind(propname).unwrap(),
        WebdavPropSelection::PropName
    );

    let allprop = br#"<?xml version="1.0" encoding="utf-8" ?>
     <D:propfind xmlns:D="DAV:">
       <D:allprop/>
       <D:include>
         <D:supported-live-property-set/>
         <D:supported-report-set/>
       </D:include>
     </D:propfind>"#;
    let WebdavPropSelection::AllProp(include) = parse_propfind(allprop).unwrap() else {
        panic!("not allprop");
    };
    assert_eq!(include[1], WebdavPropName::SupportedReportSet);

    assert_eq!(
        parse_propfind(b"").unwrap(),
        WebdavPropSelection::AllProp(vec![])
    );
    assert_eq!(
        parse_propfind(b"<D:propfind xmlns:D=\"DAV:\"/>").unwrap_err(),
        WebdavBodyParseError::Missing("prop")
    );
    assert!(matches!(
        parse_propfind(b"<propfind/>").unwrap_err(),
        WebdavBodyParseError::UnexpectedRoot(_)
    ));
}

#[test]
fn proppatch_reads_the_rfc_4918_example() {
    let body = br#"<?xml version="1.0" encoding="utf-8" ?>
     <D:propertyupdate xmlns:D="DAV:"
             xmlns:Z="http://ns.example.com/standards/z39.50/">
       <D:set>
         <D:prop>
           <Z:Authors>
             <Z:Author>Jim Whitehead</Z:Author>
             <Z:Author>Roy Fielding</Z:Author>
           </Z:Authors>
         </D:prop>
       </D:set>
       <D:remove>
         <D:prop><Z:Copyright-Owner/></D:prop>
       </D:remove>
     </D:propertyupdate>"#;
    let updates = parse_proppatch(body).unwrap();

    let WebdavPropUpdate::Set(authors) = &updates[0] else {
        panic!("not a set");
    };
    assert_eq!(authors.name.local(), "Authors");
    assert!(authors.xml.starts_with(
        "<Z:Authors xmlns:Z=\"http://ns.example.com/standards/z39.50/\" xmlns:D=\"DAV:\">"
    ));
    assert!(authors.xml.ends_with("</Z:Authors>"));
    assert_eq!(authors.children.len(), 2);

    let WebdavPropUpdate::Remove(owner) = &updates[1] else {
        panic!("not a remove");
    };
    assert_eq!(
        (owner.ns(), owner.local()),
        ("http://ns.example.com/standards/z39.50/", "Copyright-Owner")
    );
}

#[test]
fn mkcol_and_mkcalendar_read_the_rfc_examples() {
    let mkcol = br#"<?xml version="1.0" encoding="utf-8" ?>
   <D:mkcol xmlns:D="DAV:"
                 xmlns:E="http://example.com/ns/">
     <D:set>
       <D:prop>
         <D:resourcetype>
           <D:collection/>
           <E:special-resource/>
         </D:resourcetype>
         <D:displayname>Special Resource</D:displayname>
       </D:prop>
     </D:set>
   </D:mkcol>"#;
    let props = parse_mkcol(mkcol).unwrap();
    assert_eq!(props[0].children[1].name.local, "special-resource");
    assert_eq!(props[1].text, "Special Resource");
    assert!(parse_mkcol(b"").unwrap().is_empty());

    let mkcalendar = br#"<?xml version="1.0" encoding="utf-8" ?>
   <C:mkcalendar xmlns:D="DAV:"
                 xmlns:C="urn:ietf:params:xml:ns:caldav">
     <D:set>
       <D:prop>
         <D:displayname>Lisa's Events</D:displayname>
         <C:calendar-description xml:lang="en"
   >Calendar restricted to events.</C:calendar-description>
         <C:supported-calendar-component-set>
           <C:comp name="VEVENT"/>
         </C:supported-calendar-component-set>
         <C:calendar-timezone><![CDATA[BEGIN:VCALENDAR
PRODID:-//Example Corp.//CalDAV Client//EN
END:VCALENDAR
]]></C:calendar-timezone>
       </D:prop>
     </D:set>
   </C:mkcalendar>"#;
    let props = parse_mkcalendar(mkcalendar).unwrap();
    assert_eq!(props.len(), 4);
    assert_eq!(props[0].text, "Lisa's Events");
    assert_eq!(props[1].text, "Calendar restricted to events.");
    assert_eq!(props[2].children[0].name_attr.as_deref(), Some("VEVENT"));
    assert!(props[3].text.starts_with("BEGIN:VCALENDAR\nPRODID"));
}

#[test]
fn lockinfo_reads_the_rfc_4918_example() {
    let body = br#"<?xml version="1.0" encoding="utf-8" ?>
     <D:lockinfo xmlns:D='DAV:'>
       <D:lockscope><D:exclusive/></D:lockscope>
       <D:locktype><D:write/></D:locktype>
       <D:owner>
         <D:href>http://example.org/~ejw/contact.html</D:href>
       </D:owner>
     </D:lockinfo>"#;
    let info = parse_lockinfo(body).unwrap().unwrap();
    assert_eq!(info.scope, WebdavLockScope::Exclusive);
    let owner = info.owner.unwrap();
    assert!(owner.starts_with("<D:owner xmlns:D=\"DAV:\">"));
    assert!(owner.contains("contact.html</D:href>"));

    assert_eq!(parse_lockinfo(b"").unwrap(), None);

    let shared = b"<lockinfo xmlns='DAV:'><lockscope><shared/></lockscope>\
                   <locktype><write/></locktype></lockinfo>";
    assert_eq!(
        parse_lockinfo(shared).unwrap(),
        Some(WebdavLockInfo {
            scope: WebdavLockScope::Shared,
            owner: None,
        })
    );

    let read = b"<lockinfo xmlns='DAV:'><lockscope><shared/></lockscope>\
                 <locktype><read/></locktype></lockinfo>";
    assert!(matches!(
        parse_lockinfo(read).unwrap_err(),
        WebdavBodyParseError::Invalid {
            element: "locktype",
            ..
        }
    ));

    let lock = WebdavActiveLock {
        scope: WebdavLockScope::Shared,
        depth: WebdavDepth::Zero,
        owner: None,
        timeout: Some(WebdavTimeout::Infinite),
        token: "urn:uuid:1".into(),
        root: "/a".into(),
    };
    let body = lock.lock_body();
    assert!(body.contains("<D:lockscope><D:shared/></D:lockscope><D:depth>0</D:depth>"));
    assert!(body.contains("<D:timeout>Infinite</D:timeout>"));
}

#[test]
fn reports_read_the_rfc_4791_examples() {
    let multiget = br#"<?xml version="1.0" encoding="utf-8" ?>
   <C:calendar-multiget xmlns:D="DAV:"
                        xmlns:C="urn:ietf:params:xml:ns:caldav">
     <D:prop>
       <D:getetag/>
       <C:calendar-data/>
     </D:prop>
     <D:href>/bernard/work/abcd1.ics</D:href>
     <D:href>/bernard/work/mtg1.ics</D:href>
   </C:calendar-multiget>"#;
    let WebdavReportParams::CalendarMultiget(multiget) = parse_report(multiget).unwrap() else {
        panic!("not a multiget");
    };
    assert_eq!(multiget.hrefs.len(), 2);

    let expand = br#"<?xml version="1.0" encoding="utf-8" ?>
   <C:calendar-query xmlns:D="DAV:"
                     xmlns:C="urn:ietf:params:xml:ns:caldav">
     <D:prop>
       <C:calendar-data>
         <C:expand start="20060103T000000Z"
                   end="20060105T000000Z"/>
       </C:calendar-data>
     </D:prop>
     <C:filter>
       <C:comp-filter name="VCALENDAR">
         <C:comp-filter name="VEVENT">
           <C:time-range start="20060103T000000Z"
                         end="20060105T000000Z"/>
         </C:comp-filter>
       </C:comp-filter>
     </C:filter>
     <C:timezone>BEGIN:VCALENDAR</C:timezone>
   </C:calendar-query>"#;
    let WebdavReportParams::CalendarQuery(query) = parse_report(expand).unwrap() else {
        panic!("not a query");
    };
    let data = query.data.unwrap();
    assert_eq!(
        data.expand,
        Some(CaldavTimeRange {
            start: Some("20060103T000000Z".into()),
            end: Some("20060105T000000Z".into()),
        })
    );
    assert!(data.xml.starts_with("<C:calendar-data xmlns:C="));
    assert_eq!(query.timezone.as_deref(), Some("BEGIN:VCALENDAR"));

    let freebusy = br#"<?xml version="1.0" encoding="utf-8" ?>
   <C:free-busy-query xmlns:C="urn:ietf:params:xml:ns:caldav">
     <C:time-range start="20060104T140000Z"
                     end="20060105T220000Z"/>
   </C:free-busy-query>"#;
    assert_eq!(
        parse_report(freebusy).unwrap(),
        WebdavReportParams::FreeBusyQuery(CaldavTimeRange {
            start: Some("20060104T140000Z".into()),
            end: Some("20060105T220000Z".into()),
        })
    );

    let unbounded = b"<C:free-busy-query xmlns:C='urn:ietf:params:xml:ns:caldav'>\
                      <C:time-range/></C:free-busy-query>";
    assert!(matches!(
        parse_report(unbounded).unwrap_err(),
        WebdavBodyParseError::Missing(_)
    ));
}

#[test]
fn reports_read_the_rfc_6352_example() {
    let query = br#"<?xml version="1.0" encoding="utf-8" ?>
   <C:addressbook-query xmlns:D="DAV:"
                     xmlns:C="urn:ietf:params:xml:ns:carddav">
     <D:prop>
       <D:getetag/>
       <C:address-data>
         <C:prop name="VERSION"/>
         <C:prop name="UID"/>
       </C:address-data>
     </D:prop>
     <C:filter test="anyof">
       <C:prop-filter name="FN">
         <C:text-match collation="i;unicode-casemap"
                       match-type="contains"
         >daboo</C:text-match>
       </C:prop-filter>
       <C:prop-filter name="EMAIL">
         <C:text-match collation="i;unicode-casemap"
                       match-type="contains"
         >daboo</C:text-match>
       </C:prop-filter>
     </C:filter>
   </C:addressbook-query>"#;
    let WebdavReportParams::AddressbookQuery(query) = parse_report(query).unwrap() else {
        panic!("not a query");
    };
    assert_eq!(query.filter.test, CarddavFilterTest::AnyOf);
    assert_eq!(query.filter.props.len(), 2);
    let CarddavPropCond::Match { texts, .. } = &query.filter.props[0].cond else {
        panic!("not a match");
    };
    assert_eq!(texts[0].value, "daboo");
    assert_eq!(texts[0].match_type, CarddavMatchType::Contains);
    assert!(query.data.unwrap().xml.contains("<C:prop name=\"UID\"/>"));
    assert_eq!(query.limit, None);

    let bad = b"<C:addressbook-query xmlns:D='DAV:' xmlns:C='urn:ietf:params:xml:ns:carddav'>\
                <D:prop/><C:filter test='some'/></C:addressbook-query>";
    assert!(matches!(
        parse_report(bad).unwrap_err(),
        WebdavBodyParseError::Invalid {
            element: "test",
            ..
        }
    ));
}

#[test]
fn sync_collection_reads_the_rfc_6578_example() {
    let body = br#"<?xml version="1.0" encoding="utf-8" ?>
     <D:sync-collection xmlns:D="DAV:">
       <D:sync-token>http://example.com/ns/sync/1233</D:sync-token>
       <D:sync-level>infinite</D:sync-level>
       <D:limit><D:nresults>100</D:nresults></D:limit>
       <D:prop>
         <D:getetag/>
       </D:prop>
     </D:sync-collection>"#;
    let WebdavReportParams::SyncCollection(sync) = parse_report(body).unwrap() else {
        panic!("not a sync-collection");
    };
    assert_eq!(sync.level, WebdavSyncLevel::Infinite);
    assert_eq!(sync.limit, NonZeroU32::new(100));

    let bad = b"<D:sync-collection xmlns:D='DAV:'><D:sync-token/>\
                <D:sync-level>2</D:sync-level><D:prop/></D:sync-collection>";
    assert!(matches!(
        parse_report(bad).unwrap_err(),
        WebdavBodyParseError::Invalid { .. }
    ));

    let other = b"<x:report xmlns:x='urn:x'/>";
    assert_eq!(
        parse_report(other).unwrap(),
        WebdavReportParams::Other(WebdavName {
            ns: "urn:x".into(),
            local: "report".into(),
        })
    );
    assert_eq!(
        parse_report(b"").unwrap_err(),
        WebdavBodyParseError::Missing("report")
    );
}
