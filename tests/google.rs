//! End-to-end CalDAV + CardDAV tests against Google.
//!
//! Google speaks CalDAV/CardDAV over HTTPS but only behind an OAuth2 Bearer
//! token (no app passwords), and it rejects collection creation
//! (`MKCALENDAR`/`MKCOL`), so these tests exercise item / card CRUD inside an
//! existing collection: the subject's primary calendar (whose collection is
//! named `events`) and its `default` address book. Override them with
//! `GOOGLE_CALENDAR_ID` and `GOOGLE_ADDRESSBOOK_ID`.
//!
//! There are two ways to hand a token to these tests. A token minted by hand,
//! with the `calendar` and `carddav` scopes, acts as you and dies within the
//! hour:
//!
//! ```sh
//! GOOGLE_ACCESS_TOKEN="ya29...." \
//! cargo test --test google -- --ignored
//! ```
//!
//! A Workspace service account with domain-wide delegation instead signs its
//! own assertion on behalf of a user of the domain, so the run needs no human:
//!
//! ```sh
//! GOOGLE_SERVICE_ACCOUNT_KEY_FILE=key.json \
//! GOOGLE_SERVICE_ACCOUNT_SUBJECT=google@pimalaya.org \
//! cargo test --test google -- --ignored
//! ```
//!
//! CI passes the key itself rather than a path, as
//! `GOOGLE_SERVICE_ACCOUNT_KEY`, since it comes straight out of a secret. The
//! subject defaults to `google@pimalaya.org`, the Pimalaya test user. The
//! delegation must grant the `https://www.googleapis.com/auth/calendar` and
//! `https://www.googleapis.com/auth/carddav` scopes.
//!
//! Google deviates from the RFCs in a few places the tests work around:
//!
//! - `.well-known/carddav` 301-redirects only for an authenticated PROPFIND (a
//!   plain GET 404s), so the CardDAV test starts there via
//!   [`common::google_carddav_base`]. `.well-known/caldav` does not redirect,
//!   so the CalDAV test starts at `/caldav/v2/`, which answers
//!   `current-user-principal`.
//! - A `PUT` stores the resource under a name of Google's own, so the flows
//!   carry on with the id the create answered with.
//! - The sync token is only served at Depth 0, and an empty-token initial
//!   sync is refused with a 400, so the flows sync from a Depth 0 checkpoint.
//! - CardDAV sync deltas report neither the creation nor the removal of a
//!   card made over CardDAV, and CardDAV applies a `PUT` whose `If-Match` no
//!   longer matches instead of answering 412, so the CardDAV test skips both
//!   checks. CalDAV honours both.
//! - CardDAV rewrites the vCard `UID` to its own id, so the UID query uses
//!   the UID read back from the server.

mod common;

use std::{borrow::Cow, env, fs, time::Duration};

use common::CarddavCardsChecks;

use io_oauth::{
    client::Oauth20ClientStd,
    rfc7523::{
        assertion::{Oauth20JwtBearerClaims, Oauth20JwtBearerKey},
        auth_grant::Oauth20JwtBearerGrantRequestParams,
    },
};
use pimalaya_stream::tls::Tls;
use secrecy::ExposeSecret;
use serde::Deserialize;
use url::Url;

const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";
const CARDDAV_SCOPE: &str = "https://www.googleapis.com/auth/carddav";
const DEFAULT_SUBJECT: &str = "google@pimalaya.org";

/// CalDAV event CRUD inside the subject's primary calendar.
#[test]
#[ignore = "requires GOOGLE_ACCESS_TOKEN or a service account key, and --ignored"]
fn caldav() {
    let calendar_id = env::var("GOOGLE_CALENDAR_ID").unwrap_or("events".into());

    common::caldav_items(
        "https://apidata.googleusercontent.com/caldav/v2/",
        common::bearer_auth(&token()),
        &calendar_id,
    );
}

/// CardDAV card CRUD inside the subject's `default` address book, discovered
/// via the authenticated `.well-known/carddav` redirect.
#[test]
#[ignore = "requires GOOGLE_ACCESS_TOKEN or a service account key, and --ignored"]
fn carddav() {
    let token = token();
    let addressbook_id = env::var("GOOGLE_ADDRESSBOOK_ID").unwrap_or("default".into());

    let base = common::google_carddav_base(&token);
    // NOTE: Google answers sync-collection but reports neither the creation
    // nor the removal of a card made over CardDAV, and applies a stale
    // If-Match update instead of refusing it with 412.
    common::carddav_cards(
        base.as_str(),
        common::bearer_auth(&token),
        &addressbook_id,
        CarddavCardsChecks {
            sync: false,
            if_match: false,
        },
    );
}

/// The delegated user whose calendar and contacts the tests borrow.
fn subject() -> String {
    env::var("GOOGLE_SERVICE_ACCOUNT_SUBJECT").unwrap_or_else(|_| String::from(DEFAULT_SUBJECT))
}

/// Returns an access token for the run.
///
/// `GOOGLE_ACCESS_TOKEN` short-circuits everything. Otherwise a service account
/// key, held inline in `GOOGLE_SERVICE_ACCOUNT_KEY` or at the path
/// `GOOGLE_SERVICE_ACCOUNT_KEY_FILE`, is traded for a fresh token acting as
/// the subject.
fn token() -> String {
    if let Ok(token) = env::var("GOOGLE_ACCESS_TOKEN") {
        return token;
    }

    if let Ok(key) = env::var("GOOGLE_SERVICE_ACCOUNT_KEY") {
        return mint_token(&key);
    }

    if let Ok(path) = env::var("GOOGLE_SERVICE_ACCOUNT_KEY_FILE") {
        let key = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("cannot read the service account key at {path}: {err}"));

        return mint_token(&key);
    }

    panic!(
        "set GOOGLE_ACCESS_TOKEN, or GOOGLE_SERVICE_ACCOUNT_KEY / \
         GOOGLE_SERVICE_ACCOUNT_KEY_FILE to mint one"
    );
}

/// The subset of a service account key file the JWT bearer grant needs.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    #[serde(default = "default_token_uri")]
    token_uri: String,
}

fn default_token_uri() -> String {
    String::from("https://oauth2.googleapis.com/token")
}

/// Signs a JWT bearer assertion with the service account key, on behalf of the
/// subject, and trades it for an access token (RFC 7523 section 2.1).
///
/// The scopes ride in the claims, which is Google's deviation from the RFC, and
/// io-oauth models it: the token endpoint reads them from there rather than
/// from the request body.
fn mint_token(key: &str) -> String {
    let key: ServiceAccountKey =
        serde_json::from_str(key).expect("the service account key is valid JSON");

    let signer = Oauth20JwtBearerKey::from_pkcs8_pem(&key.private_key)
        .expect("the service account key holds a PKCS#8 private key");

    let token_uri: Url = key.token_uri.parse().expect("the token URI is a valid URL");

    let mut client =
        Oauth20ClientStd::connect(token_uri, &Tls::default(), key.client_email.as_str())
            .expect("connect to the token endpoint");

    let claims = Oauth20JwtBearerClaims {
        iss: key.client_email.as_str().into(),
        sub: Some(subject().into()),
        scope: [Cow::from(CALENDAR_SCOPE), Cow::from(CARDDAV_SCOPE)]
            .into_iter()
            .collect(),
        ..Default::default()
    };

    // NOTE: iat and exp come from the clock here, in the std client; the
    // coroutine layer underneath stays clock-free.
    let assertion = client
        .sign_jwt_bearer_assertion(&signer, claims, None, Duration::from_secs(600))
        .expect("sign the assertion");

    let params = Oauth20JwtBearerGrantRequestParams {
        assertion,
        scope: Default::default(),
    };

    let response = client
        .request_jwt_bearer_grant(params)
        .expect("trade the assertion for an access token");

    match response {
        Ok(granted) => granted.access_token.expose_secret().to_owned(),
        Err(err) => panic!("the token endpoint refused the assertion: {err:?}"),
    }
}
