# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added `WebdavSendError::UnsupportedReport`, raised by `WebdavReport` when the server says it does not implement the report: the RFC 3253 §3.6 `DAV:supported-report` precondition, whatever status wraps it, plus the `405` and `501` statuses. `WebdavSyncCollectionError::UnsupportedReport` names the same refusal on the enumeration path, and `WebdavClientStdError::is_unsupported_report` recognises both.

- Added `WebdavSyncCollectionOptions`, whose `fallback` runs the enumeration as a `PROPFIND` at Depth 1 instead of the `sync-collection` REPORT, for a server implementing none of RFC 6578. It lists every member and returns no token, and parses nothing, so it enumerates past a member the server itself cannot parse. The crate implements both paths and never chooses between them.

- Added `SUPPORTED_REPORT_SET`, `SYNC_COLLECTION` and `WebdavResponseEntry::supported_reports`, plus `CaldavCalendar::supported_reports` and `CarddavAddressbook::supported_reports`, read while listing collections and cached in `WebdavClientStd::calendar_reports` and `addressbook_reports`, so the enumeration is chosen from what the server advertises rather than from a failed request.

- Added `WebdavPropChild::children`, the parser keeping property markup at any depth rather than one level.

- Added a trace of the multistatus body to the `PROPFIND` and `REPORT` coroutines. The crate documented data dumps at trace level and had none for the one body every collection read goes through, so a server answering with something unexpected could only be diagnosed by packet capture.

### Fixed

- The collection self-entry is now recognised by path, so a server spelling its hrefs as absolute URLs (RFC 4918 §14.7 allows either) no longer enters its own collection into the member spine. A `PROPFIND` enumeration always answers with that entry, where a `sync-collection` REPORT only did on some servers.

### Changed

- **BREAKING** `WebdavSyncCollection::new`, `WebdavClientStd::sync_items` and `sync_cards` take a `WebdavSyncCollectionOptions`.

- **BREAKING** `CaldavItemEnum` and `CarddavCardEnum` return a `CaldavItemEnumOk` and a `CarddavCardEnumOk`, the references now carrying whether the server truncated the listing with a 507 row. `WebdavClientStd::enum_items` and `enum_cards` follow.

## [0.2.1] - 2026-08-22

### Changed

- Trimmed the inline documentation crate-wide and rewrapped it at 80 columns. No API change.

## [0.2.0] - 2026-08-15

### Added

- Added the CalDAV read-side sync coroutines, bringing calendars level with address books.

  `CaldavItemEnum` enumerates ETags only, `CaldavItemMultiget` batch-fetches bodies (RFC 4791 §7.9), and the `enum_items`, `multiget_items` and `sync_items` client methods drive them, the last running a `sync-collection` REPORT (RFC 6578) against a calendar.

- Added `CaldavCalendar::sync_token`, requested alongside the ctag when listing calendars, so an incremental calendar sync has a checkpoint to start from.

- Added `CaldavCalendar::components`, read from and written to `supported-calendar-component-set` (RFC 4791 §5.2.3), telling whether a collection holds `VEVENT`, `VTODO` or `VJOURNAL` and declaring it on create.

- Added `WebdavPropValue`, the value side of a `PROPPATCH`, `MKCOL` or `MKCALENDAR` property set: `Text` for escaped content, `Raw` for a markup fragment such as the `<C:comp/>` children of a component set.

- Added `WebdavPropChild`, carrying a property child's `name` attribute so attribute-valued properties can be read at all.

- Added property removal to `PROPPATCH` (RFC 4918 §9.2).

  `proppatch_body` and `WebdavProppatch::new` take the properties to remove alongside the set pairs, emit a `DAV:remove` instruction for them and leave an empty instruction block out of the body. Setting was all a `PROPPATCH` could express before, so clearing a property was not expressible at all.

- Added `WebdavPropFailure` plus `WebdavResponseEntry::failures`, holding the properties a non-2xx propstat named with the status that refused them.

- Added `WebdavProppatchOk`, returned by `WebdavProppatch` in place of `()`: the parsed multistatus plus the local names the request asked to set or remove.

  `update_addressbook` and `update_calendar` verify both, failing with `PropertiesRejected` when the server refused a property and `PropertiesIgnored` when it never mentioned one, RFC 4918 §9.2.1 wanting a propstat for each. iCloud answers a PROPPATCH on a collection it does not have with a 200 propstat naming nothing, which used to read as success.

- Added `summarize_body`, which boils an error body down to one line: its DAV `responsedescription`, else an HTML `title`, else the markup stripped, the whitespace collapsed and the length capped.

- Added `CarddavAddressbookPatch`, the partial update `CarddavAddressbookUpdate` now takes.

  Each property is doubly optional, so `None` leaves it alone, `Some(None)` removes it and `Some(Some(value))` sets it. A flat `CarddavAddressbook` cannot tell "leave alone" from "remove", which is why a cleared property used to vanish on the way to the wire.

### Changed

- Bumped io-http to 0.5. The coroutines take and yield its types, so a consumer bumps in step for a single version to resolve.

- Raised the minimum supported Rust version from 1.87 to 1.88, following pimalaya-stream and io-http.

- Bumped pimalaya-stream from 0.1 to 0.3, whose `Read` and `Write` retry a stream reporting it is not ready. **Behaviour change.**

  A blocking socket is not supposed to report `EAGAIN`, yet callers saw one end an exchange with a bare `Resource temporarily unavailable`, macOS especially. The transport now retries for a minute before giving up with a `TimedOut` naming the budget, and arms a read deadline at connect time so a silent server stops blocking the caller forever.

  Its `StreamStd` is renamed `stream::Stream` and its connects take a per-transport options struct, which is what this crate now calls.

- Collapsed `CarddavCardRef`/`CarddavCardEntry` to a single verbatim `id` (the href's last path segment), stopped `CarddavCardCreate` appending `.vcf`, and renamed `CarddavCardUpdateOk.uri` to `id`.

  **Breaking**: read `id` instead of `uri`, and pass any extension yourself on create (`create_card(book, "alice.vcf", …)`).

- Gave calendar items the verbatim `id` cards already had: `join_path` no longer appends `.ics` and a listing no longer strips it.

  **Breaking**: pass the whole resource name to every item verb (`create_item(cal, "event-1.ics", …)`), and drop any extension juggling around the listed id.

- Moved `CaldavItemEntry` from `rfc4791::item::list` to `rfc4791::item`, beside the new `CaldavItemRef`, mirroring `rfc6352::card`.

  **Breaking**: import `rfc4791::item::CaldavItemEntry`.

- Property sets take a `WebdavPropValue` instead of a `&str`, so `property_set`, `proppatch_body`, `mkcol_body`, `prop_set_body`, `mkcalendar_body`, `WebdavProppatch::new` and `WebdavMkcol::new` all changed shape.

  **Breaking**: wrap text values in `WebdavPropValue::Text`.

- `proppatch_body` and `WebdavProppatch::new` take the properties to remove as a second list, and no longer share their body builder with the creation requests: `prop_set_body` stays set-only, `MKCOL` and `MKCALENDAR` having nothing to remove.

  **Breaking**: pass `&[]` to keep the previous set-only behaviour.

- `WebdavSendError::HttpStatus` and `WebdavFollowRedirectsError::HttpStatus` are struct variants (`{ status, body }`) rendering a summary of the body rather than the body itself, so a Fastmail 404 no longer prints its whole HTML page. The raw body is unchanged on the value.

  **Breaking**: destructure with `HttpStatus { status, body }`.

- `CarddavAddressbookUpdate::new` and `WebdavClient::update_addressbook` take a `CarddavAddressbookPatch` instead of a `CarddavAddressbook`, and `property_set` is now only the `MKCOL` side.

  **Breaking**: build a patch, wrapping each value you were setting in `Some(Some(…))`. Fields left `None` keep their server value, so an update that only sets properties behaves identically, and a caller that read the collection back to refill unchanged fields can drop that round-trip.

- `WebdavPropItem::children` holds `WebdavPropChild` values rather than bare local names.

  **Breaking**: read `child.local` where a `String` was read before.

- Renamed every public type to the Pimalaya naming canon, domain then target then verb: the WebDAV core takes the `Webdav` prefix, the CalDAV layer `Caldav`, the CardDAV layer `Carddav`. Coroutines stopped being verb-first, so `ListCalendars` became `CaldavCalendarList`.

  **Breaking**: every import changes. Module paths are untouched, so only the type names move.

### Fixed

- Fixed card `read`/`update`/`delete` addressing the wrong resource on `.vcf`-suffixing servers (Fastmail, iCloud): the id is verbatim end-to-end, so a listed id round-trips through every verb.

- Fixed `card create` returning an unusable id when the server names the resource itself (Google): `CarddavCardCreateOk.id` now comes from the `Location` header when present, else the caller's name.

- Fixed item `read`/`update`/`delete` addressing the wrong resource: a listed id had `.ics` stripped while every verb re-appended it, so any id not ending in `.ics` addressed nothing.

- Fixed `item create` returning an unusable id when the server names the resource itself, the way `card create` was fixed.

- Fixed `CaldavCalendar::tz` being read by a listing but never written back, so a time zone was silently dropped on create and update.

- Fixed the item listing keeping a calendar's own multistatus self-entry as a bogus item (iCloud echoes the collection), matching the card listing.

- Fixed the copyright holder and year in both license files, and the supported version line in the security policy.

- Fixed the live provider suites leaking collections and resources into real accounts: teardown was written as the last steps of each flow, so any failure skipped it. Every flow now tears down on the failing path too, and reports what it could not remove.

### Removed

- Removed the docs/ folder, replaced by cairn/ following the Cairn convention (spec, changes, log) with its AGENTS.md activation stanza.

## [0.1.0] - 2026-07-16

### Added

- Added the I/O-free `WebdavCoroutine` and the `webdav_try!` macro (the coroutine equivalent of `?`).

  The trait pairs a `Yield` and a `Return` associated type with a two-variant `WebdavCoroutineState`. Standard coroutines pick the shared `WebdavYield` (`WantsRead` / `WantsWrite`); the discovery ones declare their own `WebdavRedirectYield`, whose `WantsRedirect` variant surfaces a 3xx to the caller instead of following it.

- Added I/O-free WebDAV core coroutines following RFC 4918: `PROPFIND`, `PROPPATCH`, `MKCOL`, `COPY`, `MOVE`, `DELETE`, `GET`, `PUT`, `OPTIONS` and `REPORT`, the low-level send coroutine, the `WebdavAuth` modes (Basic, Bearer) and a multistatus parser resolving entity references and carrying the top-level sync-token and response-level status rows (RFC 6578).

- Added I/O-free CalDAV coroutines following RFC 4791: calendar collection list, create, update and delete, calendar object resource (item) read, create, update and delete, and calendar home-set discovery.

- Added I/O-free CardDAV coroutines following RFC 6352: address book collection list, create, update and delete, contact card read, create, update and delete, `addressbook-multiget` batch fetch, ETag-only enumeration, and address book home-set discovery.

  Cards are addressed by their server-returned resource name rather than a reconstructed `<id>.vcf`, so servers that do not suffix `.vcf` no longer trip spurious `If-Match` 412s.

- Added the I/O-free current-user-principal discovery coroutine following RFC 5397.

- Added the collection synchronization coroutine following RFC 6578: a `sync-collection` REPORT returning changed and vanished rows, the next sync token and a truncation flag, with a dedicated invalid-sync-token error driving the full-enumeration fallback.

- Added the `client` cargo feature enabling the std-blocking `WebdavClientStd`.

  A light client wrapping any `Read + Write` stream, exposing one method per WebDAV operation plus the cached discovery flow, and opening `http://` and `https://` URLs itself under one of the TLS features (`rustls-ring` default, `rustls-aws`, `native-tls`).

  It owns a single connected stream and never follows redirects, surfacing the target URL in `WebdavClientStdError::UnexpectedRedirect` so the caller can reconnect via `set_stream`. Its public `stream` lets a higher-level crate pump its own coroutines against that stream while reusing the discovery cache.

- Added offline test suites resuming every coroutine and client method against scripted HTTP responses, reaching 100% line coverage (cargo-tarpaulin, LLVM engine), plus ignored live-provider suites for Radicale, Stalwart, Fastmail, Google and iCloud.

[unreleased]: https://github.com/pimalaya/io-webdav/compare/v0.2.1..HEAD
[0.2.1]: https://github.com/pimalaya/io-webdav/compare/v0.2.0..v0.2.1
[0.2.0]: https://github.com/pimalaya/io-webdav/compare/v0.1.0..v0.2.0
[0.1.0]: https://github.com/pimalaya/io-webdav/compare/root..v0.1.0
