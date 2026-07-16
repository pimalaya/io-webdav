//! List the calendars of a CalDAV account via the std-blocking
//! [`WebdavClientStd`].
//!
//! Requires one of the TLS feature flags (`rustls-ring`, `rustls-aws` or
//! `native-tls`) so the client can open `https://` URLs end-to-end via
//! [`pimalaya_stream`].
//!
//! # Usage
//!
//! ```sh
//! WEBDAV_URL=https://dav.example.org/ \
//!   WEBDAV_USERNAME=alice \
//!   WEBDAV_PASSWORD=secret \
//!   cargo run --example list_calendars
//! ```

use std::env;

use io_http::rfc7617::basic::HttpAuthBasic;
use io_webdav::{client::WebdavClientStd, rfc4918::WebdavAuth};
use pimalaya_stream::tls::Tls;
use url::Url;

fn main() {
    env_logger::init();

    let url: Url = env::var("WEBDAV_URL")
        .expect("WEBDAV_URL env var")
        .parse()
        .expect("valid WEBDAV_URL");

    let username = env::var("WEBDAV_USERNAME").expect("WEBDAV_USERNAME env var");
    let password = env::var("WEBDAV_PASSWORD").expect("WEBDAV_PASSWORD env var");
    let auth = WebdavAuth::Basic(HttpAuthBasic::new(username, password));

    let mut client = WebdavClientStd::connect(&url, &Tls::default(), auth).unwrap();
    client.calendar_home_set().unwrap();

    for calendar in client.list_calendars().unwrap() {
        println!("{}: {:?}", calendar.id, calendar.display_name);
    }
}
