# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1] - Unreleased

### Added

- Added the I/O-free `WebdavCoroutine` and the `webdav_try!` macro (the coroutine equivalent of `?`).

  The trait pairs a `Yield` and a `Return` associated type with a two-variant `WebdavCoroutineState`. Standard coroutines pick the shared `WebdavYield` (`WantsRead` / `WantsWrite`); the redirect-capable discovery coroutines declare their own `WebdavRedirectYield`, adding a `WantsRedirect { url, keep_alive, same_origin }` variant that surfaces a 3xx to the caller instead of following it.

- Added I/O-free WebDAV core coroutines following RFC 4918: `PROPFIND`, `PROPPATCH`, `MKCOL`, `COPY`, `MOVE`, `DELETE`, `GET`, `PUT`, `OPTIONS` and `REPORT`, the low-level send coroutine, the `WebdavAuth` modes (Basic, Bearer) and a multistatus parser resolving entity references and carrying the top-level sync-token and response-level status rows (RFC 6578).

- Added I/O-free CalDAV coroutines following RFC 4791: calendar collection list, create, update and delete, calendar object resource (item) read, create, update and delete, and calendar home-set discovery.

- Added I/O-free CardDAV coroutines following RFC 6352: address book collection list, create, update and delete, contact card read, create, update and delete, `addressbook-multiget` batch fetch, ETag-only enumeration, and address book home-set discovery.

  Cards are addressed by their server-returned resource name rather than a reconstructed `<id>.vcf`, so servers that do not suffix `.vcf` no longer trip spurious `If-Match` 412s.

- Added the I/O-free current-user-principal discovery coroutine following RFC 5397.

- Added the collection synchronization coroutine following RFC 6578: a `sync-collection` REPORT returning changed and vanished rows, the next sync token and a truncation flag, with a dedicated invalid-sync-token error driving the full-enumeration fallback.

- Added the `client` cargo feature enabling the std-blocking `WebdavClientStd`.

  A light client wrapping any `Read + Write` stream and exposing one method per WebDAV operation plus the cached discovery flow (current-user-principal to calendar / address book home set); `connect` opens `http://` / `https://` URLs itself under one of the TLS features (`rustls-ring` default, `rustls-aws`, `native-tls`). The client owns a single connected stream and never follows redirects: it surfaces the target URL in `WebdavClientStdError::UnexpectedRedirect` so the caller can reconnect via `set_stream`. `WebdavClientStd::stream` (and the `WebdavStream` trait) let higher-level crates pump their own coroutines against the connected stream while reusing the discovery cache.

- Added offline test suites resuming every coroutine and client method against scripted HTTP responses, reaching 100% line coverage (cargo-tarpaulin, LLVM engine), plus ignored live-provider suites for Radicale, Stalwart, Fastmail, Google and iCloud.

[0.0.1]: https://github.com/pimalaya/io-webdav/releases/tag/v0.0.1
