//! Offline coverage of the CalDAV layer (RFC 4791): the calendar and
//! item vocabularies, the request-body helpers and every coroutine,
//! resumed against scripted HTTP response bytes.

mod common;

use common::*;
use io_webdav::{
    rfc4791::{
        calendar::{
            Calendar, calendar_query_body, create::CreateCalendar, delete::DeleteCalendar,
            home_set::CalendarHomeSet, list::ListCalendars, mkcalendar_body, property_set,
            update::UpdateCalendar,
        },
        item::{
            create::CreateItem, delete::DeleteItem, join_path, list::ListItems, read::ReadItem,
            update::UpdateItem,
        },
    },
    rfc4918::{DISPLAYNAME, GETETAG, WebdavAuth},
};
use url::Url;

const UA: &str = "io-webdav/test";

fn base() -> Url {
    Url::parse("https://dav.example.org/").unwrap()
}

// --- vocabulary and body helpers -----------------------------------------

#[test]
fn property_set_keeps_only_the_present_fields() {
    let calendar = Calendar {
        id: "personal".into(),
        display_name: Some("Personal".into()),
        color: Some("#ff0000".into()),
        description: Some("Main calendar".into()),
        ..Default::default()
    };
    let set = property_set(&calendar);
    assert_eq!(set.len(), 3);
    assert_eq!(set[0], (DISPLAYNAME, "Personal"));

    assert!(property_set(&Calendar::default()).is_empty());
}

#[test]
fn mkcalendar_body_roots_at_the_caldav_element() {
    let body = mkcalendar_body(&[(DISPLAYNAME, "Work")]);
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
fn item_join_path_appends_the_ics_suffix() {
    assert_eq!(
        join_path("/dav/calendars/personal/", "/event-1"),
        "/dav/calendars/personal/event-1.ics"
    );
}

// --- calendar coroutines ---------------------------------------------------

#[test]
fn list_calendars_maps_calendar_collections_only() {
    let mut list = ListCalendars::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/");
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
            <cs:getctag>ctag-1</cs:getctag>
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
    // NOTE: the home itself (no calendar resourcetype) and the empty-id
    // root href are both skipped.
    assert_eq!(calendars.len(), 1);
    let calendar = calendars.first().unwrap();
    assert_eq!(calendar.id, "personal");
    assert_eq!(calendar.display_name.as_deref(), Some("Personal"));
    assert_eq!(calendar.description.as_deref(), Some("Main calendar"));
    assert_eq!(calendar.color.as_deref(), Some("#00ff00"));
    assert_eq!(calendar.ctag.as_deref(), Some("ctag-1"));
    assert!(calendar.tz.as_deref().unwrap().contains("VTIMEZONE"));
}

#[test]
fn create_calendar_sends_mkcalendar() {
    let calendar = Calendar {
        id: "work".into(),
        display_name: Some("Work".into()),
        ..Default::default()
    };
    let mut create =
        CreateCalendar::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/", &calendar);
    let (request, ret) = expect_exchange(&mut create, &http_response("201 Created", &[], ""));
    assert!(request.starts_with("mkcalendar /dav/calendars/work/ http/1.1\r\n"));
    assert!(request.contains("<c:mkcalendar"));
    ret.unwrap();
}

#[test]
fn update_calendar_sends_proppatch() {
    let calendar = Calendar {
        id: "work".into(),
        display_name: Some("Renamed".into()),
        ..Default::default()
    };
    let mut update =
        UpdateCalendar::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/", &calendar);
    let reply = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let (request, ret) = expect_exchange(&mut update, &reply);
    assert!(request.starts_with("proppatch /dav/calendars/work/ http/1.1\r\n"));
    assert!(request.contains("<d:displayname>renamed</d:displayname>"));
    ret.unwrap();
}

#[test]
fn delete_calendar_targets_the_collection() {
    let mut delete = DeleteCalendar::new(&base(), &WebdavAuth::None, UA, "/dav/calendars/", "work");
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("delete /dav/calendars/work/ http/1.1\r\n"));
    ret.unwrap();
}

#[test]
fn calendar_home_set_resolves_the_href() {
    let mut discovery = CalendarHomeSet::new(&base(), &WebdavAuth::None, UA, "/principals/alice/");
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
    let mut discovery = CalendarHomeSet::new(&base(), &WebdavAuth::None, UA, "/principals/alice/");
    let reply = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let (_, ret) = expect_redirect_exchange(&mut discovery, &reply);
    assert!(ret.unwrap().is_none());
}

// --- item coroutines --------------------------------------------------------

#[test]
fn list_items_maps_calendar_data_entries() {
    let mut list = ListItems::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "<C:comp-filter name=\"VEVENT\" />",
    );
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
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
        <d:href>/dav/calendars/personal/no-data.ics</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-2"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/</d:href>
        <d:propstat>
          <d:prop><c:calendar-data>X</c:calendar-data></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut list, &multistatus_response(xml));
    assert!(request.starts_with("report /dav/calendars/personal/ http/1.1\r\n"));
    assert!(request.contains("comp-filter name=\"vevent\""));

    let items = ret.unwrap();
    // NOTE: the data-less entry and the empty-id root href are skipped.
    assert_eq!(items.len(), 1);
    let item = items.first().unwrap();
    assert_eq!(item.id, "event-1");
    assert_eq!(item.etag.as_deref(), Some("etag-1"));
    assert!(item.data.starts_with(b"BEGIN:VCALENDAR"));
}

#[test]
fn read_item_returns_body_and_etag() {
    let mut read = ReadItem::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1",
    );
    let reply = http_response("200 OK", &[("ETag", "\"etag-1\"")], "BEGIN:VCALENDAR");
    let (request, ret) = expect_exchange(&mut read, &reply);
    assert!(request.starts_with("get /dav/calendars/personal/event-1.ics http/1.1\r\n"));

    let body = ret.unwrap();
    assert_eq!(body.data, b"BEGIN:VCALENDAR");
    assert_eq!(body.etag.as_deref(), Some("etag-1"));
}

#[test]
fn create_item_puts_with_if_none_match() {
    let mut create = CreateItem::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1",
        b"BEGIN:VCALENDAR".to_vec(),
    );
    let reply = http_response("201 Created", &[("ETag", "\"etag-1\"")], "");
    let (request, ret) = expect_exchange(&mut create, &reply);
    assert!(request.starts_with("put /dav/calendars/personal/event-1.ics http/1.1\r\n"));
    assert!(request.contains("if-none-match: *\r\n"));

    let ok = ret.unwrap();
    assert_eq!(ok.id, "event-1");
    assert_eq!(ok.etag.as_deref(), Some("etag-1"));
}

#[test]
fn update_item_puts_with_the_known_etag() {
    let mut update = UpdateItem::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1",
        b"BEGIN:VCALENDAR".to_vec(),
        Some("etag-1"),
    );
    // NOTE: no ETag header in the reply, so the outcome carries none.
    let (request, ret) = expect_exchange(&mut update, &http_response("204 No Content", &[], ""));
    assert!(request.contains("if-match: \"etag-1\"\r\n"));

    let ok = ret.unwrap();
    assert_eq!(ok.id, "event-1");
    assert!(ok.etag.is_none());
}

#[test]
fn delete_item_targets_the_resource() {
    let mut delete = DeleteItem::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/calendars/personal/",
        "event-1",
        Some("etag-1"),
    );
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("delete /dav/calendars/personal/event-1.ics http/1.1\r\n"));
    assert!(request.contains("if-match: \"etag-1\"\r\n"));
    ret.unwrap();
}
