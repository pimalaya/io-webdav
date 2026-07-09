//! Offline coverage of the current-user-principal discovery (RFC 5397),
//! resumed against scripted HTTP response bytes.

mod common;

use common::*;
use io_webdav::{
    rfc4918::WebdavAuth, rfc4918::follow_redirects::FollowRedirectsError,
    rfc5397::current_user_principal::CurrentUserPrincipal,
};
use url::Url;

const UA: &str = "io-webdav/test";

fn base() -> Url {
    Url::parse("https://dav.example.org/").unwrap()
}

#[test]
fn discovery_resolves_the_principal_href() {
    let mut discovery = CurrentUserPrincipal::new(&base(), &WebdavAuth::None, UA);
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/</d:href>
        <d:propstat>
          <d:prop>
            <d:current-user-principal><d:href>/principals/alice/</d:href></d:current-user-principal>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_redirect_exchange(&mut discovery, &multistatus_response(xml));
    assert!(request.starts_with("propfind / http/1.1\r\n"));
    assert!(request.contains("depth: 0\r\n"));
    assert!(request.contains("<d:current-user-principal/>"));

    let principal = ret.unwrap().expect("principal discovered");
    assert_eq!(
        principal.as_str(),
        "https://dav.example.org/principals/alice/"
    );
}

#[test]
fn discovery_yields_none_on_an_empty_multistatus() {
    let mut discovery = CurrentUserPrincipal::new(&base(), &WebdavAuth::None, UA);
    let reply = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let (_, ret) = expect_redirect_exchange(&mut discovery, &reply);
    assert!(ret.unwrap().is_none());
}

#[test]
fn discovery_surfaces_redirects() {
    let mut discovery = CurrentUserPrincipal::new(&base(), &WebdavAuth::None, UA);
    expect_redirect_wants_write(&mut discovery, None);
    expect_redirect_wants_read(&mut discovery);

    let reply = http_response(
        "301 Moved Permanently",
        &[("Location", "https://dav2.example.org/dav/")],
        "",
    );
    let (url, _, same_origin) = expect_wants_redirect(&mut discovery, &reply);
    assert_eq!(url.as_str(), "https://dav2.example.org/dav/");
    assert!(!same_origin);
}

#[test]
fn discovery_maps_failure_statuses() {
    let mut discovery = CurrentUserPrincipal::new(&base(), &WebdavAuth::None, UA);
    let (_, ret) =
        expect_redirect_exchange(&mut discovery, &http_response("401 Unauthorized", &[], ""));
    assert!(matches!(
        ret.unwrap_err(),
        FollowRedirectsError::HttpStatus(401, _)
    ));
}
