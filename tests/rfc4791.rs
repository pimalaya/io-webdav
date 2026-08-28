//! Offline coverage of the CalDAV layer (RFC 4791): the calendar and item
//! vocabularies, the request-body helpers and every coroutine, resumed against
//! scripted HTTP response bytes.

mod common;

use common::*;
use io_webdav::{
    rfc4791::{
        calendar::{
            CaldavCalendar, calendar_multiget_body, calendar_query_body,
            create::CaldavCalendarCreate, delete::CaldavCalendarDelete,
            home_set::CaldavCalendarHomeSet, list::CaldavCalendarList, mkcalendar_body,
            property_set, update::CaldavCalendarUpdate,
        },
        item::{
            create::CaldavItemCreate, delete::CaldavItemDelete, enumerate::CaldavItemEnum,
            join_path, list::CaldavItemList, multiget::CaldavItemMultiget, read::CaldavItemRead,
            update::CaldavItemUpdate,
        },
    },
    rfc4918::{DISPLAYNAME, GETETAG, WebdavAuth, WebdavPropValue, send::WebdavSendError},
};
use url::Url;

const UA: &str = "io-webdav/test";

fn base() -> Url {
    Url::parse("https://dav.example.org/").unwrap()
}

// --- vocabulary and body helpers ---

#[test]
fn property_set_keeps_only_the_present_fields() {
    let calendar = CaldavCalendar {
        id: "personal".into(),
        display_name: Some("Personal".into()),
        color: Some("#ff0000".into()),
        description: Some("Main calendar".into()),
        ..Default::default()
    };
    let set = property_set(&calendar);
    assert_eq!(set.len(), 3);
    assert_eq!(set[0], (DISPLAYNAME, WebdavPropValue::Text("Personal")));

    assert!(property_set(&CaldavCalendar::default()).is_empty());
}

#[test]
fn property_set_carries_the_time_zone_and_the_component_set() {
    // NOTE: both are read back by a listing, so both have to be writable, or a
    // calendar cannot be round-tripped through create.
    let calendar = CaldavCalendar {
        id: "personal".into(),
        components: ["VEVENT".to_string(), "VTODO".to_string()].into(),
        tz: Some("BEGIN:VTIMEZONE\r\nEND:VTIMEZONE".into()),
        ..Default::default()
    };
    let body = mkcalendar_body(&property_set(&calendar));
    let xml = core::str::from_utf8(&body).unwrap();
    assert!(xml.contains("<C:calendar-timezone>BEGIN:VTIMEZONE"));
    // NOTE: the component set is markup, not text: escaping it would leave the
    // server with a property whose value is a literal string.
    assert!(xml.contains(
        "<C:supported-calendar-component-set><C:comp name=\"VEVENT\"/><C:comp name=\"VTODO\"/></C:supported-calendar-component-set>"
    ));
}

#[test]
fn mkcalendar_body_roots_at_the_caldav_element() {
    let body = mkcalendar_body(&[(DISPLAYNAME, WebdavPropValue::Text("Work"))]);
    let xml = core::str::from_utf8(&body).unwrap();
    assert!(xml.contains("<C:mkcalendar"));
    assert!(xml.contains("<D:displayname>Work</D:displayname>"));
}

#[test]
fn calendar_query_body_nests_the_component_filter() {
    let body = calendar_query_body(&[GETETAG], "<C:comp-filter name=\"VEVENT\" />");
    let xml = core::str::from_utf8(&body).unwrap();
    assert!(xml.contains(
        "<C:filter><C:comp-filter name=\"VCALENDAR\"><C:comp-filter name=\"VEVENT\" /></C:comp-filter></C:filter>"
    ));
}

#[test]
fn calendar_multiget_body_lists_escaped_hrefs() {
    let hrefs = vec![
        "/dav/calendars/personal/event-1.ics".to_string(),
        "/dav/calendars/personal/a&b.ics".to_string(),
    ];
    let body = calendar_multiget_body(&hrefs, &[GETETAG]);
    let xml = core::str::from_utf8(&body).unwrap();
    assert!(xml.contains("<C:calendar-multiget"));
    assert!(xml.contains("<D:href>/dav/calendars/personal/event-1.ics</D:href>"));
    assert!(xml.contains("<D:href>/dav/calendars/personal/a&amp;b.ics</D:href>"));
}

#[test]
fn item_join_path_keeps_the_resource_name_verbatim() {
    assert_eq!(
        join_path("/dav/calendars/personal/", "/event-1.ics"),
        "/dav/calendars/personal/event-1.ics"
    );
    assert_eq!(
        join_path("/dav/calendars/personal", "event-1"),
        "/dav/calendars/personal/event-1"
    );
}

// --- calendar coroutines ---

#[test]
fn list_calendars_maps_calendar_collections_only() {
    let mut list = CaldavCalendarList::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/");
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"
        xmlns:cs="http://calendarserver.org/ns/" xmlns:i="http://inf-it.com/ns/ab/">
      <d:response>
        <d:href>/dav/calendars/</d:href>
        <d:propstat>
          <d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/dav/calendars/personal/</d:href>
        <d:propstat>
          <d:prop>
            <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
            <d:displayname>Personal</d:displayname>
            <c:calendar-description>Main calendar</c:calendar-description>
            <i:calendar-color>#00ff00</i:calendar-color>
            <c:supported-calendar-component-set>
              <c:comp name="VEVENT"/>
              <c:comp name="VTODO"/>
            </c:supported-calendar-component-set>
            <cs:getctag>ctag-1</cs:getctag>
            <d:sync-token>http://example.org/ns/sync/42</d:sync-token>
            <c:calendar-timezone>BEGIN:VTIMEZONE\nEND:VTIMEZONE</c:calendar-timezone>
            <d:unknown-extension>x</d:unknown-extension>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/</d:href>
        <d:propstat>
          <d:prop><d:resourcetype><d:collection/><c:calendar/></d:resourcetype></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut list, &multistatus_response(xml));
    assert!(request.starts_with("propfind /dav/calendars/ http/1.1\r\n"));
    assert!(request.contains("depth: 1\r\n"));

    let calendars = ret.unwrap();
    // NOTE: the home itself (no calendar resourcetype) and the empty-id root
    // href are both skipped.
    assert_eq!(calendars.len(), 1);
    let calendar = calendars.first().unwrap();
    assert_eq!(calendar.id, "personal");
    assert_eq!(calendar.display_name.as_deref(), Some("Personal"));
    assert_eq!(calendar.description.as_deref(), Some("Main calendar"));
    assert_eq!(calendar.color.as_deref(), Some("#00ff00"));
    assert_eq!(calendar.ctag.as_deref(), Some("ctag-1"));
    assert_eq!(
        calendar.sync_token.as_deref(),
        Some("http://example.org/ns/sync/42")
    );
    assert!(calendar.tz.as_deref().unwrap().contains("VTIMEZONE"));
    let components: Vec<&str> = calendar.components.iter().map(String::as_str).collect();
    assert_eq!(components, ["VEVENT", "VTODO"]);
}

#[test]
fn create_calendar_sends_mkcalendar() {
    let calendar = CaldavCalendar {
        id: "work".into(),
        display_name: Some("Work".into()),
        ..Default::default()
    };
    let mut create =
        CaldavCalendarCreate::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/", &calendar);
    let (request, ret) = expect_exchange(&mut create, &http_response("201 Created", &[], ""));
    assert!(request.starts_with("mkcalendar /dav/calendars/work/ http/1.1\r\n"));
    assert!(request.contains("<c:mkcalendar"));
    ret.unwrap();
}

#[test]
fn update_calendar_sends_proppatch() {
    let calendar = CaldavCalendar {
        id: "work".into(),
        display_name: Some("Renamed".into()),
        ..Default::default()
    };
    let mut update =
        CaldavCalendarUpdate::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/", &calendar);
    let reply = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let (request, ret) = expect_exchange(&mut update, &reply);
    assert!(request.starts_with("proppatch /dav/calendars/work/ http/1.1\r\n"));
    assert!(request.contains("<d:displayname>renamed</d:displayname>"));
    ret.unwrap();
}

#[test]
fn delete_calendar_targets_the_collection() {
    let mut delete =
        CaldavCalendarDelete::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/", "work");
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("delete /dav/calendars/work/ http/1.1\r\n"));
    ret.unwrap();
}

#[test]
fn calendar_home_set_resolves_the_href() {
    let mut discovery =
        CaldavCalendarHomeSet::new(&base(), &WebdavAuth::None, UA, "/principals/alice/");
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
      <d:response>
        <d:href>/principals/alice/</d:href>
        <d:propstat>
          <d:prop>
            <c:calendar-home-set><d:href>/dav/calendars/</d:href></c:calendar-home-set>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_redirect_exchange(&mut discovery, &multistatus_response(xml));
    assert!(request.starts_with("propfind /principals/alice/ http/1.1\r\n"));
    assert!(request.contains("<c:calendar-home-set/>"));

    let home = ret.unwrap().expect("home-set discovered");
    assert_eq!(home.as_str(), "https://dav.example.org/dav/calendars/");
}

#[test]
fn calendar_home_set_yields_none_on_an_empty_multistatus() {
    let mut discovery =
        CaldavCalendarHomeSet::new(&base(), &WebdavAuth::None, UA, "/principals/alice/");
    let reply = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let (_, ret) = expect_redirect_exchange(&mut discovery, &reply);
    assert!(ret.unwrap().is_none());
}

// --- item coroutines ---

const ITEMS_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/dav/calendars/personal/event-1.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag-1"</d:getetag>
        <c:calendar-data>BEGIN:VCALENDAR
END:VCALENDAR</c:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/personal/event-2</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag-2"</d:getetag>
        <c:calendar-data>BEGIN:VCALENDAR
END:VCALENDAR</c:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/personal/no-data.ics</d:href>
    <d:propstat>
      <d:prop><d:getetag>"etag-3"</d:getetag></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/personal/</d:href>
    <d:propstat>
      <d:prop><c:calendar-data>BEGIN:VCALENDAR</c:calendar-data></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href></d:href>
    <d:propstat>
      <d:prop><c:calendar-data>BEGIN:VCALENDAR</c:calendar-data></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

#[test]
fn list_items_maps_calendar_data_entries() {
    let mut list = CaldavItemList::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "<C:comp-filter name=\"VEVENT\" />",
    );

    let (request, ret) = expect_exchange(&mut list, &multistatus_response(ITEMS_XML));
    assert!(request.starts_with("report /dav/calendars/personal/ http/1.1\r\n"));
    assert!(request.contains("comp-filter name=\"vevent\""));

    let items = ret.unwrap();
    // NOTE: the data-less entry, the collection self-entry and the empty href
    // are all skipped. Every surviving id is the href's last segment verbatim,
    // no `.ics` stripped, so `event-1.ics` stays `event-1.ics` and the
    // suffix-less `event-2` stays `event-2`.
    assert_eq!(items.len(), 2);
    let first = items.iter().find(|item| item.id == "event-1.ics").unwrap();
    assert_eq!(first.etag.as_deref(), Some("etag-1"));
    assert!(first.data.starts_with(b"BEGIN:VCALENDAR"));
    let second = items.iter().find(|item| item.id == "event-2").unwrap();
    assert_eq!(second.etag.as_deref(), Some("etag-2"));
}

#[test]
fn enum_items_returns_etag_only_references() {
    let mut enumerate = CaldavItemEnum::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "",
    );
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/calendars/personal/event-1.ics</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut enumerate, &multistatus_response(xml));
    assert!(request.contains("<d:getetag/>"));
    assert!(!request.contains("calendar-data"));

    let refs = ret.unwrap();
    assert_eq!(refs.refs.len(), 1);
    let first = refs.refs.first().unwrap();
    // NOTE: the id is the href's last segment verbatim, `.ics` included
    assert_eq!(first.id, "event-1.ics");
    assert_eq!(first.etag.as_deref(), Some("etag-1"));
}

#[test]
fn enum_items_skips_the_collection_self_entry_and_empty_hrefs() {
    // NOTE: iCloud echoes the calendar collection itself (its href ends in a
    // slash) in the calendar-query response; it must not enter the spine as a
    // bogus item named after the collection. An href with no last segment at
    // all yields no addressable id either.
    let mut enumerate = CaldavItemEnum::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/17170244959/calendars/work/",
        "",
    );
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/17170244959/calendars/work/</d:href>
        <d:propstat>
          <d:prop><d:getetag>"coll-etag"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href></d:href>
        <d:propstat>
          <d:prop><d:getetag>"empty-href"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/17170244959/calendars/work/5d18175a.ics</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (_request, ret) = expect_exchange(&mut enumerate, &multistatus_response(xml));

    let refs = ret.unwrap();
    assert_eq!(refs.refs.len(), 1);
    assert_eq!(refs.refs.first().unwrap().id, "5d18175a.ics");
}

#[test]
fn multiget_items_requests_each_href() {
    let mut multiget = CaldavItemMultiget::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        &["event-1.ics", "event-2"],
    );
    let (request, ret) = expect_exchange(&mut multiget, &multistatus_response(ITEMS_XML));
    assert!(request.starts_with("report /dav/calendars/personal/ http/1.1\r\n"));
    assert!(request.contains("depth: 0\r\n"));
    assert!(request.contains("<d:href>/dav/calendars/personal/event-1.ics</d:href>"));
    assert!(request.contains("<d:href>/dav/calendars/personal/event-2</d:href>"));

    let items = ret.unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn read_item_returns_body_and_etag() {
    let mut read = CaldavItemRead::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1.ics",
    );
    let reply = http_response("200 OK", &[("ETag", "\"etag-1\"")], "BEGIN:VCALENDAR");
    let (request, ret) = expect_exchange(&mut read, &reply);
    assert!(request.starts_with("get /dav/calendars/personal/event-1.ics http/1.1\r\n"));

    let body = ret.unwrap();
    assert_eq!(body.data, b"BEGIN:VCALENDAR");
    assert_eq!(body.etag.as_deref(), Some("etag-1"));
}

#[test]
fn create_item_uses_the_id_verbatim() {
    // NOTE: the id is the resource name. io-webdav never appends `.ics`, so a
    // bare `event-1` is PUT at `.../event-1`, not `.../event-1.ics`. The caller
    // owns the whole name and picks its own extension.
    let mut create = CaldavItemCreate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1",
        b"BEGIN:VCALENDAR".to_vec(),
    );
    // NOTE: no `Location` in the reply → the returned id falls back to the
    // caller's name.
    let reply = http_response("201 Created", &[("ETag", "\"etag-1\"")], "");
    let (request, ret) = expect_exchange(&mut create, &reply);
    assert!(request.starts_with("put /dav/calendars/personal/event-1 http/1.1\r\n"));
    assert!(request.contains("if-none-match: *\r\n"));
    assert!(request.contains("content-type: text/calendar; charset=utf-8\r\n"));

    let ok = ret.unwrap();
    assert_eq!(ok.id, "event-1");
    assert_eq!(ok.etag.as_deref(), Some("etag-1"));
}

#[test]
fn create_item_prefers_the_location_id_when_the_server_relocates() {
    // NOTE: a server may store the item under a name of its own and report it
    // in `Location`: the returned id is then that name, not the caller's, while
    // the PUT still targets the caller's name.
    let mut create = CaldavItemCreate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "client-name.ics",
        b"BEGIN:VCALENDAR".to_vec(),
    );
    let reply = http_response(
        "201 Created",
        &[
            ("ETag", "\"etag-1\""),
            (
                "Location",
                "https://dav.example.org/dav/calendars/personal/server-9f8e7d.ics",
            ),
        ],
        "",
    );
    let (request, ret) = expect_exchange(&mut create, &reply);
    assert!(request.starts_with("put /dav/calendars/personal/client-name.ics http/1.1\r\n"));

    let ok = ret.unwrap();
    assert_eq!(ok.id, "server-9f8e7d.ics");
    assert_eq!(ok.etag.as_deref(), Some("etag-1"));
}

#[test]
fn update_item_puts_with_the_known_etag() {
    let mut update = CaldavItemUpdate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1.ics",
        b"BEGIN:VCALENDAR".to_vec(),
        Some("etag-1"),
    );
    // NOTE: no ETag header in the reply, so the outcome carries none.
    let (request, ret) = expect_exchange(&mut update, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("put /dav/calendars/personal/event-1.ics http/1.1\r\n"));
    assert!(request.contains("if-match: \"etag-1\"\r\n"));

    let ok = ret.unwrap();
    assert_eq!(ok.id, "event-1.ics");
    assert!(ok.etag.is_none());
}

#[test]
fn create_and_update_name_a_refused_duplicate_uid() {
    // NOTE: RFC 4791 §5.3.2 names the element and only recommends the status,
    // so the element is what says the collection already holds the UID.
    const REFUSAL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
    <d:error xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
      <c:no-uid-conflict><d:href>/dav/calendars/personal/other.ics</d:href></c:no-uid-conflict>
      <d:responsedescription>UID already in use</d:responsedescription>
    </d:error>"#;

    let mut create = CaldavItemCreate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1.ics",
        b"BEGIN:VCALENDAR".to_vec(),
    );
    let (_, ret) = expect_exchange(&mut create, &http_response("409 Conflict", &[], REFUSAL));

    let err = ret.unwrap_err();
    assert!(matches!(
        err,
        WebdavSendError::DuplicateUid { status: 409, .. }
    ));
    let message = err.to_string();
    assert!(message.contains("already holds a resource with the same UID"));
    assert!(message.contains("UID already in use"));

    let mut update = CaldavItemUpdate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1.ics",
        b"BEGIN:VCALENDAR".to_vec(),
        Some("etag-1"),
    );
    let (_, ret) = expect_exchange(&mut update, &http_response("409 Conflict", &[], REFUSAL));
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSendError::DuplicateUid { status: 409, .. }
    ));

    // NOTE: a 409 carrying no precondition is any of the other conflicts a
    // write meets, and one merely spelling the words in its description is
    // none: the element is what classifies, not the text.
    let quoted = r#"<d:error xmlns:d="DAV:">
      <d:responsedescription>this is not a no-uid-conflict</d:responsedescription>
    </d:error>"#;
    let mut create = CaldavItemCreate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1.ics",
        b"BEGIN:VCALENDAR".to_vec(),
    );
    let (_, ret) = expect_exchange(&mut create, &http_response("409 Conflict", &[], quoted));
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSendError::HttpStatus { status: 409, .. }
    ));

    // NOTE: a server is free to wrap the precondition in another status, which
    // the element outlives.
    let mut update = CaldavItemUpdate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1.ics",
        b"BEGIN:VCALENDAR".to_vec(),
        None,
    );
    let (_, ret) = expect_exchange(
        &mut update,
        &http_response("507 Insufficient Storage", &[], REFUSAL),
    );
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSendError::DuplicateUid { status: 507, .. }
    ));

    // NOTE: a failure that never carried a status passes through untouched,
    // there being no precondition to read out of it.
    let mut create = CaldavItemCreate::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1.ics",
        b"BEGIN:VCALENDAR".to_vec(),
    );
    let redirect = http_response(
        "302 Found",
        &[("Location", "https://elsewhere.example.org/")],
        "",
    );
    let (_, ret) = expect_exchange(&mut create, &redirect);
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSendError::UnexpectedRedirect
    ));
}

#[test]
fn delete_item_targets_the_resource() {
    let mut delete = CaldavItemDelete::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1.ics",
        Some("etag-1"),
    );
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("delete /dav/calendars/personal/event-1.ics http/1.1\r\n"));
    assert!(request.contains("if-match: \"etag-1\"\r\n"));
    ret.unwrap();
}

#[test]
fn a_listed_item_id_round_trips_through_read() {
    // NOTE: a listed id must address the very resource the server enumerated,
    // with no extension added or stripped in between. That asymmetry broke
    // read, update and delete on every server.
    let mut list = CaldavItemList::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "",
    );
    let (_request, ret) = expect_exchange(&mut list, &multistatus_response(ITEMS_XML));
    let items = ret.unwrap();
    let second = items.iter().find(|item| item.id == "event-2").unwrap();

    let mut read = CaldavItemRead::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        &second.id,
    );
    let reply = http_response("200 OK", &[("ETag", "\"etag-2\"")], "BEGIN:VCALENDAR");
    let (request, ret) = expect_exchange(&mut read, &reply);
    assert!(request.starts_with("get /dav/calendars/personal/event-2 http/1.1\r\n"));
    ret.unwrap();
}

#[test]
fn list_calendars_reads_the_supported_report_set() {
    // NOTE: the whole point of reading it while listing is that a consumer picks
    // its enumeration from what the server advertises, rather than from a
    // REPORT that has already failed.
    let mut list = CaldavCalendarList::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/");
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
      <d:response>
        <d:href>/dav/calendars/personal/</d:href>
        <d:propstat>
          <d:prop>
            <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
            <d:supported-report-set>
              <d:supported-report><d:report><c:calendar-multiget/></d:report></d:supported-report>
              <d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>
            </d:supported-report-set>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut list, &multistatus_response(xml));
    assert!(request.contains("<d:supported-report-set/>"));

    let calendars = ret.unwrap();
    let calendar = calendars.first().unwrap();
    assert_eq!(calendar.supported_reports.len(), 2);
    assert!(calendar.supported_reports.contains("sync-collection"));
}

#[test]
fn enum_items_flags_a_truncated_listing() {
    let mut enumerate = CaldavItemEnum::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "",
    );
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/calendars/personal/event-1.ics</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/dav/calendars/personal/</d:href>
        <d:status>HTTP/1.1 507 Insufficient Storage</d:status>
      </d:response>
    </d:multistatus>"#;

    let (_request, ret) = expect_exchange(&mut enumerate, &multistatus_response(xml));

    let refs = ret.unwrap();
    assert_eq!(refs.refs.len(), 1);
    assert!(refs.truncated);
}
