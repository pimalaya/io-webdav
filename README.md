# I/O WebDAV [![Documentation](https://img.shields.io/docsrs/io-webdav?style=flat&logo=docs.rs&logoColor=white)](https://docs.rs/io-webdav/latest/io_webdav) [![Matrix](https://img.shields.io/badge/chat-%23pimalaya-blue?style=flat&logo=matrix&logoColor=white)](https://matrix.to/#/#pimalaya:matrix.org) [![Mastodon](https://img.shields.io/badge/news-%40pimalaya-blue?style=flat&logo=mastodon&logoColor=white)](https://fosstodon.org/@pimalaya)

WebDAV client library, written in Rust.

This library is composed of 3 feature-gated layers:

- Low-level **I/O-free** coroutines: these `no_std`-compatible state machines contain the whole WebDAV logic and can be used anywhere
- Mid-level **light client**: a standard, blocking WebDAV client using a `Stream: Read + Write`
- High-level **full client**: light client + TCP connections and TLS negotiations handled for you

## Table of contents

- [Features](#features)
- [RFC coverage](#rfc-coverage)
- [Usage](#usage)
  - [Coroutines](#coroutines)
  - [Light client](#light-client)
  - [Full client](#full-client)
- [AI disclosure](#ai-disclosure)
- [License](#license)
- [Social](#social)
- [Sponsoring](#sponsoring)

## Features

- **I/O-free** coroutines: `no_std` state machines; no sockets, no async runtime, no `std` required, drive against any blocking, async, or fuzz harness.
- Light standard, blocking client (requires `client` feature)
- Full standard, blocking client with **TLS** support:
  - [Rustls](https://crates.io/crates/rustls) with ring crypto (requires `rustls-ring` feature)
  - [Rustls](https://crates.io/crates/rustls) with aws crypto (requires `rustls-aws` feature)
  - [Native TLS](https://crates.io/crates/native-tls) (requires `native-tls` feature)
- **CalDAV** calendars and items (requires `rfc4791` feature)
- **CardDAV** addressbooks and cards (requires `rfc6352` feature)
- **HTTP Auth mechanisms**: `BASIC`, `BEARER`

> [!TIP]
> I/O WebDAV is written in [Rust](https://www.rust-lang.org/) and uses [cargo features](https://doc.rust-lang.org/cargo/reference/features.html) to gate backend support. The default feature set is declared in [Cargo.toml](./Cargo.toml) or on [docs.rs](https://docs.rs/crate/io-webdav/latest/features).

## RFC coverage

| Module | What it covers                                                                                                       |
|--------|----------------------------------------------------------------------------------------------------------------------|
| [4918] | WebDAV core: `PROPFIND`, `PROPPATCH`, `MKCOL`, `COPY`, `MOVE`, `DELETE`, `GET`, `PUT`, `OPTIONS`, multistatus parsing |
| [4791] | CalDAV: calendar collections and calendar object resources (items)                                                   |
| [5397] | WebDAV current principal: `current-user-principal` discovery                                                          |
| [6352] | CardDAV: addressbook collections and address object resources (cards)                                                |
| [6764] | Service discovery: `.well-known/caldav` and `.well-known/carddav` bootstrap                                          |

[4918]: https://www.rfc-editor.org/rfc/rfc4918
[4791]: https://www.rfc-editor.org/rfc/rfc4791
[5397]: https://www.rfc-editor.org/rfc/rfc5397
[6352]: https://www.rfc-editor.org/rfc/rfc6352
[6764]: https://www.rfc-editor.org/rfc/rfc6764

## Usage

I/O WebDAV can be consumed three ways, depending on how much of the I/O stack you want to own. Each mode is gated by cargo features.

Whichever mode you pick, every standard-shape coroutine implements the `WebdavCoroutine` trait with two associated types: `Yield` (intermediate progress) and `Return` (terminal value, by convention `Result<Output, Error>`). Its `resume(arg: Option<&[u8]>)` method returns a `WebdavCoroutineState<Yield, Return>` with two variants:

- `Yielded(Yield)`: intermediate yield. Most coroutines pick the standard `WebdavYield` with `WantsRead` / `WantsWrite(Vec<u8>)`. Pass `Some(&[])` after `WantsRead` to signal EOF.
- `Complete(Return)`: terminal yield, carrying `Ok(Output)` on success or `Err(Error)` on failure.

The redirect-capable discovery coroutines (`CurrentUserPrincipal`, `CalendarHomeSet`, `AddressbookHomeSet`, `FollowRedirects`) declare their own `WebdavRedirectYield` which extends the standard variants with `WantsRedirect { url, keep_alive, same_origin }`: the server responded with a 3xx and the caller chooses whether to open a new connection to `url` and retry, or surface the redirect as an error.

### Coroutines

No features required: works in `#![no_std]`, no sockets, no async runtime. You own the loop and the bytes; the library only produces request bytes and consumes server responses.

Discover the current user principal against a blocking rustls socket:

```rust,no_run
use std::{io::{Read, Write}, net::TcpStream, sync::Arc};

use io_webdav::{
    coroutine::{WebdavCoroutine, WebdavCoroutineState},
    rfc4918::{WebdavAuth, coroutine::WebdavRedirectYield},
    rfc5397::current_user_principal::CurrentUserPrincipal,
};
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;
use secrecy::SecretString;
use url::Url;

let base_url = Url::parse("https://dav.example.org/").unwrap();
let auth = WebdavAuth::Basic {
    username: "alice".into(),
    password: SecretString::from("secret"),
};

let config = ClientConfig::with_platform_verifier().unwrap();
let server_name = base_url.host_str().unwrap().to_string().try_into().unwrap();
let conn = ClientConnection::new(Arc::new(config), server_name).unwrap();
let tcp = TcpStream::connect((base_url.host_str().unwrap(), 443)).unwrap();
let mut stream = StreamOwned::new(conn, tcp);

let mut coroutine = CurrentUserPrincipal::new(&base_url, &auth, "io-webdav");
let mut arg: Option<&[u8]> = None;
let mut buf = [0u8; 8192];
let mut read_buf = Vec::<u8>::new();

let principal = loop {
    match coroutine.resume(arg.take()) {
        WebdavCoroutineState::Complete(Ok(principal)) => break principal,
        WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
        WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRead) => {
            let n = stream.read(&mut buf).unwrap();
            read_buf.clear();
            read_buf.extend_from_slice(&buf[..n]);
            arg = Some(&read_buf);
        }
        WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsWrite(bytes)) => {
            stream.write_all(&bytes).unwrap();
        }
        WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRedirect { url, .. }) => {
            todo!("reconnect to {url}");
        }
    }
};

println!("Principal: {principal:?}");
```

### Light client

Enable the `client` feature. `WebdavClientStd::new(stream, auth, base_url)` wraps any blocking `Read + Write` and exposes one method per WebDAV operation, plus the cached discovery flow (`current_user_principal` → `calendar_home_set` / `addressbook_home_set`). You still open the TCP socket and run TLS yourself, and hand over a ready-to-talk stream; the client takes it from there.

```toml,ignore
[dependencies]
io-webdav = { version = "0.0.1", default-features = false, features = ["client", "rfc4791"] }
```

```rust,no_run
use std::{net::TcpStream, sync::Arc};

use io_webdav::{client::WebdavClientStd, rfc4918::WebdavAuth};
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;
use secrecy::SecretString;
use url::Url;

let base_url = Url::parse("https://dav.example.org/").unwrap();
let auth = WebdavAuth::Basic {
    username: "alice".into(),
    password: SecretString::from("secret"),
};

let config = ClientConfig::with_platform_verifier().unwrap();
let server_name = base_url.host_str().unwrap().to_string().try_into().unwrap();
let conn = ClientConnection::new(Arc::new(config), server_name).unwrap();
let tcp = TcpStream::connect((base_url.host_str().unwrap(), 443)).unwrap();
let stream = StreamOwned::new(conn, tcp);

let mut client = WebdavClientStd::new(stream, auth, base_url);
client.calendar_home_set().unwrap();
for calendar in client.list_calendars().unwrap() {
    println!("{}: {:?}", calendar.id, calendar.display_name);
}
```

### Full client

Enable one of the TLS feature flags: `rustls-ring` (default), `rustls-aws`, or `native-tls`. `WebdavClientStd::connect(url, tls, auth)` opens `http://` / `https://` URLs via [pimalaya/stream](https://github.com/pimalaya/stream).

```toml,ignore
[dependencies]
io-webdav = "0.0.1" # rustls-ring, rfc4791 and rfc6352 are enabled by default
```

```rust,no_run
use io_webdav::{client::WebdavClientStd, rfc4918::WebdavAuth};
use pimalaya_stream::tls::Tls;
use secrecy::SecretString;
use url::Url;

let base_url = Url::parse("https://dav.example.org/").unwrap();
let auth = WebdavAuth::Basic {
    username: "alice".into(),
    password: SecretString::from("secret"),
};
let tls = Tls::default();

let mut client = WebdavClientStd::connect(&base_url, &tls, auth).unwrap();
client.calendar_home_set().unwrap();
for calendar in client.list_calendars().unwrap() {
    println!("{}: {:?}", calendar.id, calendar.display_name);
}
```

When discovery surfaces a different authority than where you first connected (an RFC 6764 `.well-known` redirect, a cross-origin home-set), use `WebdavClientStd::set_stream` to swap in a new transport.

## AI disclosure

This project is developed with AI assistance. This section documents how, so users and downstream packagers can make informed decisions.

- **Tools**: Claude Code (Anthropic), Opus 4.8, invoked locally with a persistent project-scoped memory and a small set of repo-specific rules.

- **Used for**: Refactors, mechanical multi-file edits, boilerplate (feature gates, error enums, derive macros, trait impls), test scaffolding, doc polish, exploratory design conversations.

- **Not used for**: Engineering, critical code, git manipulation (commit, merge, rebase…), real-world tests.

- **Verification**: Every AI-assisted change is read, compiled, tested, and formatted before commit (`nix develop --command cargo check / cargo test / cargo fmt`). Behavioural correctness is verified against the relevant RFC or upstream spec, not assumed from the model output. Tests are never adjusted to fit AI-generated code; the code is adjusted to fit correct behaviour.

- **Limitations**: AI models occasionally produce code that compiles and passes tests but is subtly wrong: off-by-one errors, missed edge cases, plausible but nonexistent APIs, stale RFC references. The verification workflow catches most of this; it does not catch all of it. Bug reports are welcome and taken seriously.

- **Last reviewed**: 10/06/2026

## License

This project is licensed under either of:

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

## Social

- Chat on [Matrix](https://matrix.to/#/#pimalaya:matrix.org)
- News on [Mastodon](https://fosstodon.org/@pimalaya) or [RSS](https://fosstodon.org/@pimalaya.rss)
- Mail at [pimalaya.org@posteo.net](mailto:pimalaya.org@posteo.net)

## Sponsoring

[![nlnet](https://nlnet.nl/logo/banner-160x60.png)](https://nlnet.nl/)

Special thanks to the [NLnet foundation](https://nlnet.nl/) and the [European Commission](https://www.ngi.eu/) that have been financially supporting the project for years:

- 2022 → 2023: [NGI Assure](https://nlnet.nl/project/Himalaya/)
- 2023 → 2024: [NGI Zero Entrust](https://nlnet.nl/project/Pimalaya/)
- 2024 → 2026: [NGI Zero Core](https://nlnet.nl/project/Pimalaya-PIM/)
- *2027 in preparation…*

If you appreciate the project, feel free to donate using one of the following providers:

[![GitHub](https://img.shields.io/badge/-GitHub%20Sponsors-fafbfc?logo=GitHub%20Sponsors)](https://github.com/sponsors/soywod)
[![Ko-fi](https://img.shields.io/badge/-Ko--fi-ff5e5a?logo=Ko-fi&logoColor=ffffff)](https://ko-fi.com/soywod)
[![Buy Me a Coffee](https://img.shields.io/badge/-Buy%20Me%20a%20Coffee-ffdd00?logo=Buy%20Me%20A%20Coffee&logoColor=000000)](https://www.buymeacoffee.com/soywod)
[![Liberapay](https://img.shields.io/badge/-Liberapay-f6c915?logo=Liberapay&logoColor=222222)](https://liberapay.com/soywod)
[![thanks.dev](https://img.shields.io/badge/-thanks.dev-000000?logo=data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjQuMDk3IiBoZWlnaHQ9IjE3LjU5NyIgY2xhc3M9InctMzYgbWwtMiBsZzpteC0wIHByaW50Om14LTAgcHJpbnQ6aW52ZXJ0IiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciPjxwYXRoIGQ9Ik05Ljc4MyAxNy41OTdINy4zOThjLTEuMTY4IDAtMi4wOTItLjI5Ny0yLjc3My0uODktLjY4LS41OTMtMS4wMi0xLjQ2Mi0xLjAyLTIuNjA2di0xLjM0NmMwLTEuMDE4LS4yMjctMS43NS0uNjc4LTIuMTk1LS40NTItLjQ0Ni0xLjIzMi0uNjY5LTIuMzQtLjY2OUgwVjcuNzA1aC41ODdjMS4xMDggMCAxLjg4OC0uMjIyIDIuMzQtLjY2OC40NTEtLjQ0Ni42NzctMS4xNzcuNjc3LTIuMTk1VjMuNDk2YzAtMS4xNDQuMzQtMi4wMTMgMS4wMjEtMi42MDZDNS4zMDUuMjk3IDYuMjMgMCA3LjM5OCAwaDIuMzg1djEuOTg3aC0uOTg1Yy0uMzYxIDAtLjY4OC4wMjctLjk4LjA4MmExLjcxOSAxLjcxOSAwIDAgMC0uNzM2LjMwN2MtLjIwNS4xNTYtLjM1OC4zODQtLjQ2LjY4Mi0uMTAzLjI5OC0uMTU0LjY4Mi0uMTU0IDEuMTUxVjUuMjNjMCAuODY3LS4yNDkgMS41ODYtLjc0NSAyLjE1NS0uNDk3LjU2OS0xLjE1OCAxLjAwNC0xLjk4MyAxLjMwNXYuMjE3Yy44MjUuMyAxLjQ4Ni43MzYgMS45ODMgMS4zMDUuNDk2LjU3Ljc0NSAxLjI4Ny43NDUgMi4xNTR2MS4wMjFjMCAuNDcuMDUxLjg1NC4xNTMgMS4xNTIuMTAzLjI5OC4yNTYuNTI1LjQ2MS42ODIuMTkzLjE1Ny40MzcuMjYuNzMyLjMxMi4yOTUuMDUuNjIzLjA3Ni45ODQuMDc2aC45ODVabTE0LjMxNC03LjcwNmgtLjU4OGMtMS4xMDggMC0xLjg4OC4yMjMtMi4zNC42NjktLjQ1LjQ0NS0uNjc3IDEuMTc3LS42NzcgMi4xOTVWMTQuMWMwIDEuMTQ0LS4zNCAyLjAxMy0xLjAyIDIuNjA2LS42OC41OTMtMS42MDUuODktMi43NzQuODloLTIuMzg0di0xLjk4OGguOTg0Yy4zNjIgMCAuNjg4LS4wMjcuOTgtLjA4LjI5Mi0uMDU1LjUzOC0uMTU3LjczNy0uMzA4LjIwNC0uMTU3LjM1OC0uMzg0LjQ2LS42ODIuMTAzLS4yOTguMTU0LS42ODIuMTU0LTEuMTUydi0xLjAyYzAtLjg2OC4yNDgtMS41ODYuNzQ1LTIuMTU1LjQ5Ny0uNTcgMS4xNTgtMS4wMDQgMS45ODMtMS4zMDV2LS4yMTdjLS44MjUtLjMwMS0xLjQ4Ni0uNzM2LTEuOTgzLTEuMzA1LS40OTctLjU3LS43NDUtMS4yODgtLjc0NS0yLjE1NXYtMS4wMmMwLS40Ny0uMDUxLS44NTQtLjE1NC0xLjE1Mi0uMTAyLS4yOTgtLjI1Ni0uNTI2LS40Ni0uNjgyYTEuNzE5IDEuNzE5IDAgMCAwLS43MzctLjMwNyA1LjM5NSA1LjM5NSAwIDAgMC0uOTgtLjA4MmgtLjk4NFYwaDIuMzg0YzEuMTY5IDAgMi4wOTMuMjk3IDIuNzc0Ljg5LjY4LjU5MyAxLjAyIDEuNDYyIDEuMDIgMi42MDZ2MS4zNDZjMCAxLjAxOC4yMjYgMS43NS42NzggMi4xOTUuNDUxLjQ0NiAxLjIzMS42NjggMi4zNC42NjhoLjU4N3oiIGZpbGw9IiNmZmYiLz48L3N2Zz4=)](https://thanks.dev/soywod)
[![PayPal](https://img.shields.io/badge/-PayPal-0079c1?logo=PayPal&logoColor=ffffff)](https://www.paypal.com/paypalme/soywod)
