# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1] - Unreleased

### Fixed

- Resolved entity references in the multistatus parser.

  quick-xml emits `&amp;`-style references as standalone `GeneralRef` events, which the parser silently dropped: a `displayname` of `A &amp; B` parsed as `A  B`. Predefined (`amp`, `lt`, `gt`, `quot`, `apos`) and numeric character references now resolve to their character; unknown entities are kept verbatim.
- Every request body now emits the DAV namespace with the `D` prefix (the form the RFC examples use and every interoperable client sends) instead of declaring it as the XML default namespace. Strict servers (iCloud, Google) answer HTTP 400 to an addressbook-multiget whose CardDAV-prefixed root carries default-namespace DAV children, while lenient ones (fastmail, posteo) accepted both; the all-prefixed form passes everywhere.
- Addressed existing cards by their server-returned resource name instead of reconstructing `<id>.vcf`.

  `CardEntry` and `CardRef` now carry `uri`, the raw last path segment of the href, next to the display `id` (uri with `.vcf` stripped); `ReadCard`, `UpdateCard`, `DeleteCard` and `MultigetCards` take that uri verbatim (`UpdateCardOk.id` renamed to `uri`), and `join_path` no longer appends `.vcf`. Servers are not required to suffix `.vcf` (SabreDAV-hosted cards created by webmail clients often have none), so the reconstruction PUT a nonexistent path and every `If-Match` update failed with a spurious 412. Creation still names new resources `<id>.vcf` inside `CreateCard`.

### Changed

- Carried the redirect target URL in `WebdavClientStdError::UnexpectedRedirect`, so callers can see where the server pointed.
- Removed the dead `follow_redirects` and `max_redirects` client options along with `WebdavClientStdError::TooManyRedirects`.

  The client never followed redirects (the flag was read by nobody, and the counter reset on every operation so the limit could only trip at zero); redirect resolution lives in io-http's send coroutine, and this client owns a single connected stream, so only the caller can reconnect to the target and retry (via `set_stream`), exactly like io-http's `HttpClientStd`.

### Added

- Initial release: WebDAV (RFC 4918), CalDAV (RFC 4791) and CardDAV (RFC 6352) coroutines + std client.
- Added offline test suites (tests/rfc4918, rfc4791, rfc5397, rfc6352, rfc6578, client) resuming every coroutine and client method against scripted HTTP responses, reaching 100% line coverage; coverage is measured with cargo-tarpaulin (LLVM engine) locally and in CI, uploaded to Codecov.
- Exposed `WebdavClientStd::stream` (and the `WebdavStream` trait) so higher-level crates can pump their own `WebdavCoroutine`s against the connected stream while reusing the client's discovery cache (mirrors io-jmap's public stream).
- Added the top-level `sync-token` and the response-level `status` to the multistatus parser, so `sync-collection` removal (404) and truncation (507) rows survive as entries (RFC 6578).
- Added the ctag and sync-token checkpoint properties to `ListAddressbooks` and the `Addressbook` type (mirrors the calendar ctag mapping).
- Added the `SyncCollection` coroutine (RFC 6578 `sync-collection` REPORT) returning a `SyncDelta` (changed, vanished, next token, truncated flag), with a dedicated `InvalidSyncToken` error for full-enumeration fallback.
- Added the `MultigetCards` coroutine (RFC 6352 §8.7 `addressbook-multiget` REPORT) batch-fetching card bodies by id in one round-trip.
- Added the `EnumCards` coroutine enumerating card references (`CardRef`: id plus ETag, no body) via an ETag-only `addressbook-query`.
- Added the `enum_cards`, `multiget_cards` and `sync_cards` client methods.

[0.0.1]: https://github.com/pimalaya/io-webdav/releases/tag/v0.0.1
