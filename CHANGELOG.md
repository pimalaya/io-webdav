# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.14.0] - 2026-11-24

### Added

- **BREAKING**: Added `unreadable` to `rfc6578::sync_collection::WebdavSyncDelta`, the member rows the server answered with an error, as `rfc4918::typed::WebdavUnreadableEntry`.

  Both enumerations, the `sync-collection` REPORT and its `PROPFIND` fallback, dropped a row with a status other than 2xx, 404 and 507 (a 403, a 5xx), so a full listing read as the whole collection took the member for removed. A 2xx row whose `getetag` was refused stays a change with no ETag.

## [0.13.0] - 2026-11-21

### Changed

- Moved to io-http 0.9; `client` enables `io-http/client`.

## [0.12.0] - 2026-11-20

### Added

- Added the `server` feature and its `server` module, the server side without a session (no_std, alloc), for a server that owns its HTTP stack:

  - request body parsers: `propfind::parse_propfind`, `proppatch::parse_proppatch` (set values kept as raw XML for dead properties), `mkcol::parse_mkcol` (RFC 5689), `mkcol::parse_mkcalendar`, `lock::parse_lockinfo` and `report::parse_report` (calendar-multiget, calendar-query with its whole filter, free-busy-query, addressbook-multiget, addressbook-query into the client's `CarddavFilter`, sync-collection);
  - header parsers in `header`: `Depth`, `Destination`, `Overwrite`, `If`, `If-Match` and `If-None-Match`, `Timeout`, `Lock-Token`;
  - writers: `multistatus::WebdavMultistatusBody` (propstats grouped by status, response-level status, sync token) and `precondition::WebdavPrecondition` (`DAV:error` bodies of RFC 4918, 5689, 3253, 4791, 6352 and 6578);
  - `name::WebdavPropName`, the DAV, CalDAV, CardDAV, CalendarServer and Apple iCal properties typed, any other kept by name.

### Changed

- **BREAKING**: Renamed the `client` feature, which gated the blocking client, to `std`.

  A consumer naming it moves to the new name:

  ```toml
  # before
  io-webdav = { version = "0.11", default-features = false, features = ["client"] }
  # after
  io-webdav = { version = "0.12", default-features = false, features = ["std"] }
  ```

- **BREAKING**: Changed the `client` feature to gate the coroutines (`coroutine`, `rfc5397`, `rfc6578`, every method submodule of `rfc4918`, `rfc4791` and `rfc6352`, `WebdavAuth`, `emit_header`, `read_etag`) and the io-http dependency.

  It is on by default and implied by `std` and the TLS features. A consumer turning default features off to drive the coroutines itself adds it:

  ```toml
  # before
  io-webdav = { version = "0.11", default-features = false }
  # after
  io-webdav = { version = "0.12", default-features = false, features = ["client"] }
  ```

  The vocabulary stays compiled without it: namespaces, property constants, hrefs and ids, the multistatus parsers and the request body builders.

## [0.11.0] - 2026-11-15

### Added

- **BREAKING**: Added `href` to `rfc4791::item::CaldavItemRef`, `CaldavItemEntry`, `rfc6352::card::CarddavCardRef` and `CarddavCardEntry`, the `<href>` a listing, an enumeration or a multiget served.

  Escapes and raw bytes are kept as served: a `My%20Cal` and a raw `My Cal`, both read as the id `My Cal`, stay apart for a store keyed on the server's spelling.

- **BREAKING**: Added `href` to `rfc4791::calendar::CaldavCalendar` and `rfc6352::addressbook::CarddavAddressbook`, the `<href>` a listing served; a create ignores it.

- **BREAKING**: Added `href` to `CaldavItemCreateOk`, `CaldavItemUpdateOk`, `CaldavItemMoveOk`, `CarddavCardCreateOk` and `CarddavCardUpdateOk`.

  The `Location` header as served when it names the resource, else the path of the URL sent, the `Destination` for a move; the returned id is unchanged.

- Added `client::WebdavClientStd::run`, running a coroutine the caller builds over the client stream with the resend every method gets.

## [0.10.0] - 2026-11-15

### Added

- Added `rfc4791::calendar::CaldavCalendarPatch`, the partial update `CaldavCalendarUpdate` takes.

  Each of the display name, description, color and time zone is doubly optional: `None` leaves it alone, `Some(None)` removes it, `Some(Some(value))` sets it.

- Added `rfc4791::calendar::property_updates`.

- Added `rfc4791::calendar::APPLE_ICAL` and `rfc4791::calendar::INFIT_CALENDAR_COLOR`.

- Added namespace-qualified fields to `rfc4918::typed::parse_multistatus_as`.

  A struct field named in Clark notation, `{uri}local`, reads only the properties of that namespace, so two properties sharing a local name no longer collide.

### Changed

- **BREAKING**: Changed `CaldavCalendarUpdate::new` and `WebdavClientStd::update_calendar` to take a `CaldavCalendarPatch`.

  A cleared property is removed in the same `PROPPATCH`. The component set is no longer sent on update, a server protecting it once the calendar exists; `property_set` is the `MKCALENDAR` side only.

- **BREAKING**: Changed every id read from an href or a sent URL to be percent-decoded when the decoding encodes back to the segment.

  Listings, enumerations, multigets, their unreadable rows, `rfc4918::collection_id`, `rfc4918::id_from_location` and the id a create, update or move returns: a collection created as `My Book` lists as `My Book`, no longer `My%20Book`. A segment whose decoding would not be sent back as served (a `%2F`, an escape left in the decoding, `%40`, lowercase hex) keeps its served form. An id read before still addresses its resource.

- **BREAKING**: Changed `WebdavResponseEntry::id` and `WebdavResponseOf::id` to return `Cow<str>`.

### Fixed

- Fixed a calendar's color being written and read as the inf-it `calendar-color`, the address book namespace, which iCloud ignores.

  `rfc4791::calendar::CALENDAR_COLOR` is Apple's (`http://apple.com/ns/ical/`), written on create and update; a listing reads it, the inf-it one only when it is absent.

## [0.9.1] - 2026-11-15

### Added

- Added `rfc4918::check_collection_id` and `rfc4918::check_resource_id`.

  Each refuses an id that is empty or holds an empty, `.` or `..` segment, percent-encoded or `\`-separated forms included.

- Added `rfc4918::WebdavIdError`, the refused id.

### Fixed

- Fixed `WebdavClientStd` sending a request for an id addressing the server root, the home set or a parent of either.

  Every method checks its collection, item and card ids first and fails with an `InvalidInput` I/O error carrying a `WebdavIdError`, sending nothing.

- Fixed card and item listings and multigets trimming the body they read, which cut the final CRLF of every vCard and iCalendar object a server escapes as `&#13;` (iCloud, Fastmail).

  The whole text of `address-data` and `calendar-data` is returned, so a body read by a REPORT matches the one a GET returns.

- Fixed `rfc4918::collection_path` and the item and card `join_path` sending a relative id as given.

  Each byte a path segment cannot hold raw is percent-encoded (a `\` no longer reads as a separator, a multiget href is encoded too), and an escape already there is kept, so an id read from a listing is sent unchanged.

## [0.9.0] - 2026-11-15

### Added

- Added `rfc4918::typed::WebdavUnreadableEntry`, a listed resource that came without its data.

- Added `CarddavCardListOk::unreadable`.

  A card listing returns every card it could not read, a 403 or 404 row or a refused or empty `address-data`, instead of dropping it.

### Changed

- **BREAKING**: Changed `CaldavItemList` to return `CaldavItemListOk`, carrying `items`, `truncated` and `unreadable`.

  It returned the bare set of items, dropping the rows it could not read and the 507 row of a truncated result.

- **BREAKING**: Changed `CarddavCardMultiget` and `CaldavItemMultiget` to return `CarddavCardMultigetOk` and `CaldavItemMultigetOk`, carrying the entries and `unreadable`.

  A multiget naming a card or an item the server answers 404 says so.

- **BREAKING**: Changed `WebdavClientStd::list_items`, `multiget_items` and `multiget_cards` to return the new outputs.

- **BREAKING**: Changed the pimalaya-stream dependency to 0.4.

  Its `tls::Tls` moved the trusted certificate from `cert` to `server`, and gained `client`, the certificate presented when the server asks for one (mutual TLS).

- **BREAKING**: Changed the io-http dependency to 0.8.

## [0.8.0] - 2026-11-13

### Added

- Added `rfc4918::encode_id`.

  It percent-encodes a name the caller mints into a path segment, the form an id has.

- Added `rfc4918::WebdavMemberFailure`.

### Changed

- **BREAKING** Changed `rfc4918::parse_multistatus` and `rfc4918::typed::parse_multistatus_as` to fail on a body that is not one whole `multistatus` document.

  An HTML page, a cut or malformed document and an empty body no longer parse as a shorter listing, so PROPFIND, REPORT, PROPPATCH and discovery fail on them. Text outside the root is ignored.

- **BREAKING** Changed `WebdavMultistatusParseError` to carry `UnexpectedRoot` and `Incomplete`.

- **BREAKING** Changed `WebdavDelete`, `WebdavCopy`, `WebdavMove` and `CaldavItemMove` to fail on a 207 naming failed members.

  The members come back in `WebdavSendError::MemberFailures`.

- **BREAKING** Changed `WebdavSendError::UnexpectedRedirect` to carry `url`, `keep_alive` and `same_origin`.

- **BREAKING** Changed `WebdavSendRaw` and `WebdavFollowRedirects` to refuse a request whose header value carries a control character.

  Nothing is sent, and `InvalidHeader`, new on `WebdavSendError` and `WebdavFollowRedirectsError`, names the header.

- **BREAKING** Changed the id a create, update or move returns to the last segment of the URL sent, percent-encoded.

  It was the caller's string as given, which differs from the id a listing returns as soon as the name needs an escape.

- **BREAKING** Changed the io-http dependency to 0.7.

  The coroutines take and yield its types, so a consumer bumps in step for a single version to resolve. `WebdavSendError::Send` and `WebdavFollowRedirectsError::Send` carry its new `InvalidRequest`, `UnsupportedTransferCoding` and `TooManyInterimResponses` failures.

- Changed `WebdavClientStd` to keep the trailing slash on calendar and address book request paths.

- Changed the multistatus parsers to keep a character reference to NUL or with a sign verbatim.

### Fixed

- Fixed a weak entity tag losing its closing quote in `rfc4918::read_etag` and in listed ETags.

  A weak tag is kept whole and sent back whole in `If-Match` and `If-None-Match`.

- Fixed `WebdavRequest::if_match` and `if_none_match` sending a bare value starting with `W/` unquoted.

- Fixed `rfc4918::parse_multistatus` reading `response`, `propstat` and `prop` elements nested in a property value as part of the envelope.

- Fixed `CaldavCalendarHomeSet` and `CarddavAddressbookHomeSet` joining several home set hrefs into one URL.

  The first href is returned.

- Fixed `WebdavSyncCollection` missing a `valid-sync-token` precondition carried by a status other than 403.

- Fixed `WebdavReport` and `WebdavSyncCollection` taking a refusal that quotes a precondition name for the precondition.

- Fixed `WebdavSyncCollection` listing the collection itself when the server percent-encodes its path differently.

## [0.7.0] - 2026-11-04

### Added

- Added `rfc4918::typed::parse_multistatus_as` and `WebdavMultistatusOf`.

  The 2xx properties of each response deserialize into a serde type of the caller, matched by local name; a response whose properties do not fit fails the parse.

### Changed

- **BREAKING** Changed `WebdavPropfind` and `WebdavReport` to parse into `M`, `WebdavMultistatus` by default or `WebdavMultistatusOf<P>`.

  A call site that never names the result type annotates the coroutine: `let report: WebdavReport = WebdavReport::new(..)`.

- **BREAKING** Changed `WebdavMultistatusParseError` to carry `InvalidProps`.

- **BREAKING** Changed `WebdavSendRaw` and `WebdavFollowRedirects` to move the response body into `WebdavSendOk::body` rather than copy it.

  `WebdavSendOk::response` keeps the status line and headers, its `body` left empty.

- Changed the multistatus parser to a scanner built for WebDAV bodies, replacing xmlparser.

  The output is unchanged and the parse runs 3.4 to 9 times faster. Past the HTTP exchange, a CalDAV or CardDAV listing, multiget or sync spends 4.4 to 9 times less per body.

## [0.6.0] - 2026-11-04

### Added

- Added `rfc4791::item::r#move::CaldavItemMove` and `WebdavClientStd::move_item`.

  They move an item into another calendar of the same server with `MOVE`, keeping its id. A refused `UID` surfaces as `WebdavSendError::DuplicateUid`.

- Added `WebdavClientStd::set_reconnect`.

  A client given a way to reconnect resends a request, once, when the server closed the connection before answering it. `connect` sets it, `set_stream` drops it.

- Added `rfc4918::collection_id` and `rfc4918::collection_path`.

### Changed

- **BREAKING** Changed the id of a listed calendar or address book outside the home set to its href's path.

  An id starting with a slash addresses that path, and a collection directly under the home set keeps its last segment. Google calendars, all listed as `/caldav/v2/<calendar>/events/`, no longer collapse into one named `events`.

- **BREAKING** Changed `rfc4918::parse_multistatus` to return `Result<WebdavMultistatus, WebdavMultistatusParseError>`.

  It fails on elements nested deeper than `MULTISTATUS_MAX_DEPTH` (64). `WebdavSendError` and `WebdavFollowRedirectsError` gained the matching `Multistatus` variant.

- **BREAKING** Bumped io-http to 0.6.

  The coroutines take and yield its types, so a consumer bumps in step for a single version to resolve.

- Changed the minimum Rust version to 1.89.

### Fixed

- Fixed the request following a closed connection failing.

  A client from `connect` now reconnects and resends it, after a `Connection: close` (Radicale after a PUT) or an idle keep-alive timeout.

- Fixed `rfc4918::parse_multistatus` running in quadratic time.

  Closed subtrees are moved instead of cloned into their parent, and about 35 KB of nested elements no longer overflows a 1 MiB stack.

## [0.5.1] - 2026-10-02

### Fixed

- Fixed `is_duplicate_uid` missing the refusal Fastmail sends: a write refused with RFC 6638's `CALDAV:unique-scheduling-object-resource` precondition, the UID already held by another event of the user, now surfaces as `WebdavSendError::DuplicateUid` too, whatever the status (Fastmail answers 403).

## [0.5.0] - 2026-10-01

### Changed

- **BREAKING** Changed `WebdavClientStd::connect` to take `WebdavClientStdConnectOptions` after the credential, replacing the `&Tls` argument.

  Its `proxy` routes the connection through a SOCKS5 or HTTP proxy; the default resolves it from the environment, as before.

## [0.4.1] - 2026-09-29

### Fixed

- Fixed `no_std` builds pulling in `std` ([#1]).

  quick-xml was replaced by xmlparser, which builds without `std`. Parsing is unchanged.

## [0.4.0] - 2026-09-26

### Added

- Added the `rfc6352::filter` module, modelling the RFC 6352 §10.5 query filter.

  `CarddavFilter` holds prop-filters, param-filters, is-not-defined and text-matches with every attribute. Its default is the empty `allof`, which matches every card.

- Added `CarddavCardListOptions`, filtering and capping a card listing.

  Its `filter` is a `CarddavFilter`, and its `limit` is sent as the RFC 6352 §8.6.1 `C:limit`.

- **BREAKING** Added `WebdavSendError::UnsupportedFilter`.

  Raised when a listing is refused with the `supported-filter` or `supported-collation` precondition (RFC 6352 §8.6), matched on the element whatever status wraps it.

- Added `rfc4918::escape_attr`, escaping a double-quoted attribute value.

### Changed

- **BREAKING** Changed `CarddavCardList::new` and `WebdavClientStd::list_cards` to take a `CarddavCardListOptions`.

  `CarddavCardListOptions::default()` keeps the match-all listing.

- **BREAKING** Changed the card listing to return `CarddavCardListOk`, the twin of `CarddavCardEnumOk`.

  Its `truncated` flag reports the 507 row a server answers when it caps the result.

- **BREAKING** Changed `addressbook_query_body` to take the filter and the limit.

### Fixed

- Fixed a card listing reading as complete when the server truncated it.

  The 507 row names the collection itself, which the self-entry skip discarded.

## [0.3.0] - 2026-08-31

### Added

- **BREAKING** Added `WebdavSendError::DuplicateUid`, raised when the server refuses a create or update because the collection already holds that `UID`.

  The `CALDAV:no-uid-conflict` (RFC 4791 §5.3.2) or `CARDDAV:no-uid-conflict` (RFC 6352 §6.3.2) precondition, read as an element out of the body, whatever status wraps it. One variant serves both flavours, the caller knowing which one it called, and the crate neither retries nor renames. `WebdavClientStdError::is_duplicate_uid` recognises it, the peer of `is_unsupported_report`. A new variant on a public error enum breaks an exhaustive match.

- Added `WebdavSendError::UnsupportedReport`, raised by `WebdavReport` when the server says it does not implement the report.

  The RFC 3253 §3.6 `DAV:supported-report` precondition, whatever status wraps it, plus the `405` and `501` statuses. `WebdavSyncCollectionError::UnsupportedReport` names the same refusal on the enumeration path, and `WebdavClientStdError::is_unsupported_report` recognises both.

- Added `WebdavSyncCollectionOptions`, whose `fallback` enumerates with a `PROPFIND` at Depth 1 instead of the `sync-collection` REPORT.

  For a server implementing none of RFC 6578. It lists every member, returns no token and parses nothing, so it enumerates past a member the server itself cannot parse. The crate implements both paths and never chooses between them.

- Added the supported-report reads: `SUPPORTED_REPORT_SET`, `SYNC_COLLECTION`, `WebdavResponseEntry::supported_reports`, `CaldavCalendar::supported_reports` and `CarddavAddressbook::supported_reports`.

  They are read while listing collections and cached in `WebdavClientStd::calendar_reports` and `addressbook_reports`, so the enumeration is chosen from what the server advertises rather than from a failed request.

- Added `WebdavPropChild::children`, the parser keeping property markup at any depth rather than one level.

- Added a trace of the multistatus body to the `PROPFIND` and `REPORT` coroutines.

  The crate documented data dumps at trace level and had none for the one body every collection read goes through, so a server answering with something unexpected could only be diagnosed by packet capture.

### Changed

- Bumped `quick-xml` to 0.42, whose events are `str`-based rather than byte-based.

  The multistatus parser reads local names, text, character references and CDATA as strings straight from the reader, so nothing decodes UTF-8 by hand any more. No API change.

- **BREAKING** Added a `WebdavSyncCollectionOptions` parameter to `WebdavSyncCollection::new`, `WebdavClientStd::sync_items` and `sync_cards`.

- **BREAKING** Changed `CaldavItemEnum` and `CarddavCardEnum` to return a `CaldavItemEnumOk` and a `CarddavCardEnumOk`.

  The references now carry whether the server truncated the listing with a 507 row. `WebdavClientStd::enum_items` and `enum_cards` follow.

### Fixed

- Recognised the collection self-entry by path, so a server spelling its hrefs as absolute URLs no longer enters the collection into its own member spine.

  RFC 4918 §14.7 allows either spelling. A `PROPFIND` enumeration always answers with that entry, where a `sync-collection` REPORT only did on some servers.

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

[unreleased]: https://github.com/pimalaya/io-webdav/compare/v0.14.0..HEAD
[0.14.0]: https://github.com/pimalaya/io-webdav/compare/v0.13.0..v0.14.0
[0.13.0]: https://github.com/pimalaya/io-webdav/compare/v0.12.0..v0.13.0
[0.12.0]: https://github.com/pimalaya/io-webdav/compare/v0.11.0..v0.12.0
[0.11.0]: https://github.com/pimalaya/io-webdav/compare/v0.10.0..v0.11.0
[0.10.0]: https://github.com/pimalaya/io-webdav/compare/v0.9.1..v0.10.0
[0.9.1]: https://github.com/pimalaya/io-webdav/compare/v0.9.0..v0.9.1
[0.9.0]: https://github.com/pimalaya/io-webdav/compare/v0.8.0..v0.9.0
[0.8.0]: https://github.com/pimalaya/io-webdav/compare/v0.7.0..v0.8.0
[0.7.0]: https://github.com/pimalaya/io-webdav/compare/v0.6.0..v0.7.0
[0.6.0]: https://github.com/pimalaya/io-webdav/compare/v0.5.1..v0.6.0
[0.5.1]: https://github.com/pimalaya/io-webdav/compare/v0.5.0..v0.5.1
[0.5.0]: https://github.com/pimalaya/io-webdav/compare/v0.4.1..v0.5.0
[0.4.1]: https://github.com/pimalaya/io-webdav/compare/v0.4.0..v0.4.1
[0.4.0]: https://github.com/pimalaya/io-webdav/compare/v0.3.0..v0.4.0
[0.3.0]: https://github.com/pimalaya/io-webdav/compare/v0.2.1..v0.3.0
[0.2.1]: https://github.com/pimalaya/io-webdav/compare/v0.2.0..v0.2.1
[0.2.0]: https://github.com/pimalaya/io-webdav/compare/v0.1.0..v0.2.0
[0.1.0]: https://github.com/pimalaya/io-webdav/compare/root..v0.1.0

[#1]: https://github.com/pimalaya/io-webdav/issues/1
