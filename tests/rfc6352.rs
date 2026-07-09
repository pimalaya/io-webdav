//! Offline coverage of the CardDAV layer (RFC 6352): the addressbook
//! and card vocabularies, the request-body helpers and every coroutine,
//! resumed against scripted HTTP response bytes.

mod common;

use common::*;
use io_webdav::{
    rfc4918::{DISPLAYNAME, GETETAG, WebdavAuth},
    rfc6352::{
        addressbook::{
            Addressbook, addressbook_multiget_body, addressbook_query_body,
            create::CreateAddressbook, delete::DeleteAddressbook, home_set::AddressbookHomeSet,
            list::ListAddressbooks, property_set, update::UpdateAddressbook,
        },
        card::{
            create::CreateCard, delete::DeleteCard, enumerate::EnumCards, join_path,
            list::ListCards, multiget::MultigetCards, read::ReadCard, update::UpdateCard,
        },
    },
};
use url::Url;

const UA: &str = "io-webdav/test";

fn base() -> Url {
    Url::parse("https://dav.example.org/").unwrap()
}

// --- vocabulary and body helpers -----------------------------------------

#[test]
fn property_set_keeps_only_the_present_fields() {
    let addressbook = Addressbook {
        id: "contacts".into(),
        display_name: Some("Contacts".into()),
        color: Some("#0000ff".into()),
        description: Some("Main addressbook".into()),
        ..Default::default()
    };
    let set = property_set(&addressbook);
    assert_eq!(set.len(), 3);
    assert_eq!(set[0], (DISPLAYNAME, "Contacts"));

    assert!(property_set(&Addressbook::default()).is_empty());
}

#[test]
fn addressbook_query_body_carries_the_allof_filter() {
    let body = addressbook_query_body(&[GETETAG]);
    let xml = core::str::from_utf8(&body).unwrap();
    assert!(xml.contains("<C:addressbook-query"));
    assert!(xml.contains("<C:filter test=\"allof\"></C:filter>"));
}

#[test]
fn addressbook_multiget_body_lists_escaped_hrefs() {
    let hrefs = vec![
        "/dav/books/contacts/alice.vcf".to_string(),
        "/dav/books/contacts/a&b.vcf".to_string(),
    ];
    let body = addressbook_multiget_body(&hrefs, &[GETETAG]);
    let xml = core::str::from_utf8(&body).unwrap();
    assert!(xml.contains("<C:addressbook-multiget"));
    assert!(xml.contains("<D:href>/dav/books/contacts/alice.vcf</D:href>"));
    assert!(xml.contains("<D:href>/dav/books/contacts/a&amp;b.vcf</D:href>"));
}

#[test]
fn card_join_path_keeps_the_resource_name_verbatim() {
    assert_eq!(
        join_path("/dav/books/contacts/", "/alice.vcf"),
        "/dav/books/contacts/alice.vcf"
    );
    assert_eq!(
        join_path("/dav/books/contacts", "bob"),
        "/dav/books/contacts/bob"
    );
}

// --- addressbook coroutines -------------------------------------------------

#[test]
fn list_addressbooks_maps_addressbook_collections_only() {
    let mut list = ListAddressbooks::new(&base(), &WebdavAuth::None, UA, "/dav/books/");
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"
        xmlns:cs="http://calendarserver.org/ns/" xmlns:i="http://inf-it.com/ns/ab/">
      <d:response>
        <d:href>/dav/books/</d:href>
        <d:propstat>
          <d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/dav/books/contacts/</d:href>
        <d:propstat>
          <d:prop>
            <d:resourcetype><d:collection/><c:addressbook/></d:resourcetype>
            <d:displayname>Contacts</d:displayname>
            <c:addressbook-description>Main addressbook</c:addressbook-description>
            <i:addressbook-color>#0000ff</i:addressbook-color>
            <cs:getctag>ctag-1</cs:getctag>
            <d:sync-token>http://example.org/ns/sync/42</d:sync-token>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/</d:href>
        <d:propstat>
          <d:prop><d:resourcetype><d:collection/><c:addressbook/></d:resourcetype></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut list, &multistatus_response(xml));
    assert!(request.starts_with("propfind /dav/books/ http/1.1\r\n"));
    assert!(request.contains("depth: 1\r\n"));

    let addressbooks = ret.unwrap();
    // NOTE: the home itself (no addressbook resourcetype) and the
    // empty-id root href are both skipped.
    assert_eq!(addressbooks.len(), 1);
    let addressbook = addressbooks.first().unwrap();
    assert_eq!(addressbook.id, "contacts");
    assert_eq!(addressbook.display_name.as_deref(), Some("Contacts"));
    assert_eq!(addressbook.description.as_deref(), Some("Main addressbook"));
    assert_eq!(addressbook.color.as_deref(), Some("#0000ff"));
    assert_eq!(addressbook.ctag.as_deref(), Some("ctag-1"));
    assert_eq!(
        addressbook.sync_token.as_deref(),
        Some("http://example.org/ns/sync/42")
    );
}

#[test]
fn create_addressbook_sends_the_extended_mkcol() {
    let addressbook = Addressbook {
        id: "team".into(),
        display_name: Some("Team".into()),
        ..Default::default()
    };
    let mut create =
        CreateAddressbook::new(&base(), &WebdavAuth::None, UA, "/dav/books/", &addressbook);
    let (request, ret) = expect_exchange(&mut create, &http_response("201 Created", &[], ""));
    assert!(request.starts_with("mkcol /dav/books/team/ http/1.1\r\n"));
    assert!(request.contains("<d:resourcetype><d:collection/><c:addressbook/></d:resourcetype>"));
    ret.unwrap();
}

#[test]
fn update_addressbook_sends_proppatch() {
    let addressbook = Addressbook {
        id: "team".into(),
        display_name: Some("Renamed".into()),
        ..Default::default()
    };
    let mut update =
        UpdateAddressbook::new(&base(), &WebdavAuth::None, UA, "/dav/books/", &addressbook);
    let reply = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let (request, ret) = expect_exchange(&mut update, &reply);
    assert!(request.starts_with("proppatch /dav/books/team/ http/1.1\r\n"));
    assert!(request.contains("<d:displayname>renamed</d:displayname>"));
    ret.unwrap();
}

#[test]
fn delete_addressbook_targets_the_collection() {
    let mut delete = DeleteAddressbook::new(&base(), &WebdavAuth::None, UA, "/dav/books/", "team");
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("delete /dav/books/team/ http/1.1\r\n"));
    ret.unwrap();
}

#[test]
fn addressbook_home_set_resolves_the_href() {
    let mut discovery =
        AddressbookHomeSet::new(&base(), &WebdavAuth::None, UA, "/principals/alice/");
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">
      <d:response>
        <d:href>/principals/alice/</d:href>
        <d:propstat>
          <d:prop>
            <c:addressbook-home-set><d:href>/dav/books/</d:href></c:addressbook-home-set>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_redirect_exchange(&mut discovery, &multistatus_response(xml));
    assert!(request.starts_with("propfind /principals/alice/ http/1.1\r\n"));
    assert!(request.contains("<c:addressbook-home-set/>"));

    let home = ret.unwrap().expect("home-set discovered");
    assert_eq!(home.as_str(), "https://dav.example.org/dav/books/");
}

#[test]
fn addressbook_home_set_yields_none_on_an_empty_multistatus() {
    let mut discovery =
        AddressbookHomeSet::new(&base(), &WebdavAuth::None, UA, "/principals/alice/");
    let reply = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let (_, ret) = expect_redirect_exchange(&mut discovery, &reply);
    assert!(ret.unwrap().is_none());
}

// --- card coroutines ---------------------------------------------------------

const CARDS_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">
  <d:response>
    <d:href>/dav/books/contacts/alice.vcf</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag-1"</d:getetag>
        <c:address-data>BEGIN:VCARD
END:VCARD</c:address-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/books/contacts/bob</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag-2"</d:getetag>
        <c:address-data>BEGIN:VCARD
END:VCARD</c:address-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/books/contacts/no-data.vcf</d:href>
    <d:propstat>
      <d:prop><d:getetag>"etag-3"</d:getetag></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/.vcf</d:href>
    <d:propstat>
      <d:prop><c:address-data>X</c:address-data></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

#[test]
fn list_cards_maps_address_data_entries() {
    let mut list = ListCards::new(&base(), &WebdavAuth::None, UA, "/dav/books/contacts/");
    let (request, ret) = expect_exchange(&mut list, &multistatus_response(CARDS_XML));
    assert!(request.starts_with("report /dav/books/contacts/ http/1.1\r\n"));
    assert!(request.contains("<c:address-data/>"));

    let cards = ret.unwrap();
    // NOTE: the data-less entry and the empty-id href are skipped; the
    // suffix-less resource name is kept verbatim.
    assert_eq!(cards.len(), 2);
    let alice = cards.iter().find(|card| card.id == "alice").unwrap();
    assert_eq!(alice.uri, "alice.vcf");
    assert_eq!(alice.etag.as_deref(), Some("etag-1"));
    assert!(alice.data.starts_with(b"BEGIN:VCARD"));
    let bob = cards.iter().find(|card| card.id == "bob").unwrap();
    assert_eq!(bob.uri, "bob");
}

#[test]
fn enum_cards_returns_etag_only_references() {
    let mut enumerate = EnumCards::new(&base(), &WebdavAuth::None, UA, "/dav/books/contacts/");
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/books/contacts/alice.vcf</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/.vcf</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-2"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut enumerate, &multistatus_response(xml));
    assert!(request.contains("<d:getetag/>"));
    assert!(!request.contains("address-data"));

    let refs = ret.unwrap();
    assert_eq!(refs.len(), 1);
    let alice = refs.first().unwrap();
    assert_eq!(alice.id, "alice");
    assert_eq!(alice.uri, "alice.vcf");
    assert_eq!(alice.etag.as_deref(), Some("etag-1"));
}

#[test]
fn multiget_cards_requests_each_href() {
    let mut multiget = MultigetCards::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        &["alice.vcf", "bob"],
    );
    let (request, ret) = expect_exchange(&mut multiget, &multistatus_response(CARDS_XML));
    assert!(request.starts_with("report /dav/books/contacts/ http/1.1\r\n"));
    assert!(request.contains("depth: 0\r\n"));
    assert!(request.contains("<d:href>/dav/books/contacts/alice.vcf</d:href>"));
    assert!(request.contains("<d:href>/dav/books/contacts/bob</d:href>"));

    let cards = ret.unwrap();
    assert_eq!(cards.len(), 2);
}

#[test]
fn read_card_returns_body_and_etag() {
    let mut read = ReadCard::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        "alice.vcf",
    );
    let reply = http_response("200 OK", &[("ETag", "\"etag-1\"")], "BEGIN:VCARD");
    let (request, ret) = expect_exchange(&mut read, &reply);
    assert!(request.starts_with("get /dav/books/contacts/alice.vcf http/1.1\r\n"));

    let body = ret.unwrap();
    assert_eq!(body.data, b"BEGIN:VCARD");
    assert_eq!(body.etag.as_deref(), Some("etag-1"));
}

#[test]
fn create_card_appends_the_vcf_suffix() {
    let mut create = CreateCard::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        "alice",
        b"BEGIN:VCARD".to_vec(),
    );
    let reply = http_response("201 Created", &[("ETag", "\"etag-1\"")], "");
    let (request, ret) = expect_exchange(&mut create, &reply);
    assert!(request.starts_with("put /dav/books/contacts/alice.vcf http/1.1\r\n"));
    assert!(request.contains("if-none-match: *\r\n"));
    assert!(request.contains("content-type: text/vcard; charset=utf-8\r\n"));

    let ok = ret.unwrap();
    assert_eq!(ok.id, "alice");
    assert_eq!(ok.etag.as_deref(), Some("etag-1"));
}

#[test]
fn update_card_uses_the_resource_name_verbatim() {
    let mut update = UpdateCard::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        "bob",
        b"BEGIN:VCARD".to_vec(),
        Some("etag-2"),
    );
    let (request, ret) = expect_exchange(&mut update, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("put /dav/books/contacts/bob http/1.1\r\n"));
    assert!(request.contains("if-match: \"etag-2\"\r\n"));

    let ok = ret.unwrap();
    assert_eq!(ok.uri, "bob");
    assert!(ok.etag.is_none());
}

#[test]
fn delete_card_targets_the_resource() {
    let mut delete = DeleteCard::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        "alice.vcf",
        None,
    );
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("delete /dav/books/contacts/alice.vcf http/1.1\r\n"));
    ret.unwrap();
}
