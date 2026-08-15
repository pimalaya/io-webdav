# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-08-15

### Added

- Added the CalDAV read-side sync coroutines, bringing calendars level with address books: `CaldavItemEnum` (ETag-only enumeration returning `CaldavItemRef` rows), `CaldavItemMultiget` (`calendar-multiget` batch fetch, RFC 4791 §7.9) and the matching `enum_items` / `multiget_items` / `sync_items` client methods, the last running a `sync-collection` REPORT (RFC 6578) against a calendar.
- Added `CaldavCalendar::sync_token`, requested alongside the existing ctag when listing calendars, so an incremental calendar sync has a checkpoint to start from.
- Added `CaldavCalendar::components`, read from and written to `supported-calendar-component-set` (RFC 4791 §5.2.3), so a caller can tell whether a collection holds `VEVENT`, `VTODO` or `VJOURNAL`, and can declare it when creating one.
- Added `WebdavPropValue`, the value side of a `PROPPATCH` / `MKCOL` / `MKCALENDAR` property set: `Text` for escaped content, `Raw` for a markup fragment such as the `<C:comp/>` children of a component set.
- Added `WebdavPropChild`, carrying a property child's `name` attribute so attribute-valued properties can be read at all.
- Added property removal to `PROPPATCH` (RFC 4918 §9.2): `proppatch_body` and `WebdavProppatch::new` take a list of properties to remove alongside the set pairs and emit a `DAV:remove` instruction for it, and an empty instruction block is left out of the body. Setting a property was the only thing a `PROPPATCH` could express before, so clearing one was not expressible at all.
- Added `WebdavPropFailure` plus `WebdavResponseEntry::failures`, holding the properties a non-2xx propstat named with the status that refused them, next to the 2xx properties the parser already kept.
- Added `WebdavProppatchOk`, returned by `WebdavProppatch` in place of `()`: the parsed multistatus plus the local names the request asked to set or remove. `WebdavClientStd::update_addressbook` and `update_calendar` now verify both, failing with `PropertiesRejected` when the server refused a property and `PropertiesIgnored` when it never mentioned one (RFC 4918 §9.2.1 wants a propstat for each). iCloud answers a PROPPATCH on a collection it does not have with a 200 propstat naming nothing, which used to read as success.
- Added `summarize_body`, which boils an error body down to one line: its DAV `responsedescription`, else an HTML `title`, else the markup stripped, whitespace collapsed and length capped.
- Added `CarddavAddressbookPatch`, the partial update `CarddavAddressbookUpdate` now takes: each property is doubly optional, so `None` leaves it alone, `Some(None)` removes it and `Some(Some(value))` sets it. `property_updates` splits a patch into the set and remove lists. A flat `CarddavAddressbook` cannot tell "leave alone" from "remove", which is why a cleared property used to vanish on the way to the wire.

### Changed

- Bumped io-http to 0.5. The coroutines take and yield its types, so a consumer bumps in step for a single version to resolve.
- Raised the minimum supported Rust version from 1.87 to 1.88, following pimalaya-stream and io-http.

- Bumped pimalaya-stream to 0.3, whose `Read` and `Write` retry a stream reporting it is not ready. **Behaviour change.**

  A blocking socket is not supposed to report `EAGAIN`, yet callers saw one surface mid-exchange and end the exchange with a bare `Resource temporarily unavailable (os error 35)`, macOS especially and the more readily the longer the exchange ran. The transport now retries such a failure for a minute before giving up with a `TimedOut` naming the budget, and arms a socket read deadline at connect time so a server going silent on a healthy connection stops blocking the caller forever. Its `StreamStd` is renamed `stream::Stream` and its connects take a per-transport options struct, which is what this crate now calls.

- Bumped pimalaya-stream to 0.2, whose only change here is the removal of its SASL module: this crate uses the blocking stream and the TLS options, neither of which moved.

- Collapsed `CarddavCardRef`/`CarddavCardEntry` to a single verbatim `id` (the href's last path segment), stopped `CarddavCardCreate` appending `.vcf`, and renamed `CarddavCardUpdateOk.uri` to `id`.

  **Breaking**: read `id` instead of `uri`, and pass any extension yourself on create (`create_card(book, "alice.vcf", …)`).

- Gave calendar items the verbatim `id` cards already had: `join_path` no longer appends `.ics` and a listing no longer strips it.

  **Breaking**: pass the whole resource name to every item verb (`create_item(cal, "event-1.ics", …)`), and drop any extension juggling around the listed id.

- Moved `CaldavItemEntry` from `rfc4791::item::list` to `rfc4791::item`, beside the new `CaldavItemRef`, mirroring `rfc6352::card`.

  **Breaking**: import `rfc4791::item::CaldavItemEntry`.

- Property sets take a `WebdavPropValue` instead of a `&str`, so `property_set`, `proppatch_body`, `mkcol_body`, `prop_set_body`, `mkcalendar_body`, `WebdavProppatch::new` and `WebdavMkcol::new` all changed shape.

  **Breaking**: wrap text values in `WebdavPropValue::Text`.

- `proppatch_body` and `WebdavProppatch::new` take the properties to remove as a second list, and no longer share their body builder with the creation requests: `prop_set_body` stays set-only, since `MKCOL` and `MKCALENDAR` have nothing to remove.

  **Breaking**: pass `&[]` to keep the previous set-only behaviour.

- `WebdavSendError::HttpStatus` and `WebdavFollowRedirectsError::HttpStatus` are struct variants (`{ status, body }`) and render a summary of the body rather than the body itself, so a Fastmail 404 no longer prints its whole HTML page and an empty body no longer leaves a trailing colon. The raw body is unchanged on the value.

  **Breaking**: destructure with `HttpStatus { status, body }`.

- `CarddavAddressbookUpdate::new` and `WebdavClient::update_addressbook` take a `CarddavAddressbookPatch` instead of a `CarddavAddressbook`. `property_set` is now only the `MKCOL` side.

  **Breaking**: build a patch, wrapping each value you were setting in `Some(Some(…))`. Fields you used to leave `None` stay `None` and keep their server value, exactly as before, so an update that only sets properties behaves identically. A caller that read the current collection to refill unchanged fields can drop that round-trip.

- `WebdavPropItem::children` holds `WebdavPropChild` values rather than bare local names.

  **Breaking**: read `child.local` where a `String` was read before.

- Renamed every public type to the Pimalaya naming canon, domain first then target then verb: the WebDAV core and the protocol-neutral pieces take the `Webdav` prefix, the CalDAV layer takes `Caldav`, the CardDAV layer takes `Carddav`. Coroutines stopped being verb-first, so `ListCalendars` became `CaldavCalendarList` and `CreateCard` became `CarddavCardCreate`.

  **Breaking**: every import changes. Module paths are untouched, so only the type names move.

### Fixed

- Fixed card `read`/`update`/`delete` addressing the wrong resource on `.vcf`-suffixing servers (Fastmail, iCloud): the id is verbatim end-to-end, so a listed id round-trips through every verb.
- Fixed `card create` returning an unusable id when the server names the resource itself (e.g. Google): `CarddavCardCreateOk.id` now comes from the `Location` header when present, else the caller's name.
- Fixed item `read`/`update`/`delete` addressing the wrong resource: a listed id had `.ics` stripped while every verb re-appended it, so any id not ending in `.ics` addressed nothing.
- Fixed `item create` returning an unusable id when the server names the resource itself: `CaldavItemCreateOk.id` now comes from the `Location` header when present, else the caller's name.
- Fixed `CaldavCalendar::tz` being read by a listing but never written back, so a time zone was silently dropped on create and update.
- Fixed the item listing keeping a calendar's own multistatus self-entry as a bogus item (iCloud echoes the collection), matching the card listing.
- Fixed the copyright holder and year in both license files, and the supported version line in the security policy.
- Fixed the live provider suites leaking collections and resources into real accounts: teardown was written as the last steps of each flow, so any failure skipped it. Every flow now tears down on the failing path too, and reports what it could not remove.

### Removed

- Removed the docs/ folder, replaced by cairn/ following the Cairn convention (spec, changes, log) with its AGENTS.md activation stanza.

## [0.1.0] - 2026-07-16

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

[unreleased]: https://github.com/pimalaya/io-webdav/compare/v0.2.0..HEAD
[0.2.0]: https://github.com/pimalaya/io-webdav/compare/v0.1.0..v0.2.0
[0.1.0]: https://github.com/pimalaya/io-webdav/compare/root..v0.1.0
