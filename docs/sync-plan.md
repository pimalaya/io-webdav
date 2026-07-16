# Sync support plan

Goal: give io-webdav everything a CardDAV consumer needs to service io-offline's sync seams (enumerate with a cursor, batch fetch, checkpoint). The push seam is already complete (create/update/delete with If-Match/If-None-Match, etags returned), so this plan is read-side only. Tasks are ordered by dependency; each is landable on its own.

## 1. Multistatus parser extensions

Files: src/rfc4918/types.rs, src/rfc4918/utils.rs.

- Add a sync_token field (Option<String>) on Multistatus, filled from the top-level DAV:sync-token element of a REPORT response (RFC 6578 section 6.2). Today the parser never reads it.
- Add a status field (Option<u16>) on ResponseEntry, filled from the response-level status element. Keep the existing behaviour of gathering props only from 2xx propstats, but stop dropping responses that have no 2xx propstat: a sync-collection removal row is an href plus a 404 response-level status and no propstat at all, and it must survive as an entry with empty props. Same for the 507 truncation row (RFC 6578 section 3.6).
- Tests: extend the parser tests with a sync-collection fixture carrying a sync-token, a changed row, a 404 removal row and a 507 row.

## 2. Checkpoint properties on addressbooks

Files: src/rfc6352/addressbook/list.rs, src/rfc6352/addressbook/types.rs, plus a shared const.

- Add a SYNC_TOKEN property const (DAV: namespace, RFC 6578 section 3) next to GETETAG in src/rfc4918/utils.rs.
- GETCTAG and the CALENDARSERVER namespace currently live in src/rfc4791/calendar/utils.rs; hoist them to rfc4918 (they are protocol-neutral CalendarServer extensions used by both CalDAV and CardDAV) and re-export from the old path or update callers.
- Request both in ListAddressbooks and add sync_token plus ctag fields (Option<String>) on the Addressbook type, mirroring how the calendar list maps GETCTAG.

## 3. sync-collection REPORT (RFC 6578)

New module: src/rfc6578/ (mod.rs plus sync_collection.rs), mirroring the module-per-RFC layout.

- A body builder sync_collection_body(sync_token: Option<&str>, props: &[Property]): sync-level 1, an empty sync-token element for an initial sync, the given props (a consumer asks for GETETAG only).
- A SyncCollection coroutine modelled on src/rfc4918/report.rs, returning a SyncDelta type: changed entries (href, etag), vanished hrefs (entries whose response status is 404), the new sync_token, and a truncated flag (a 507 row was present, meaning the consumer must run the report again from the returned token). Mind the Depth header rules of RFC 6578 section 3.3.
- A missing or rejected sync token (server returns 403 with valid-sync-token precondition) must surface as a distinct error so the consumer can fall back to a full enumerate.

## 4. addressbook-multiget REPORT (RFC 6352 section 8.7)

New file: src/rfc6352/card/multiget.rs.

- A body builder taking the href list and props (GETETAG plus ADDRESS_DATA), and a MultigetCards coroutine returning Vec<CardEntry>, modelled on the existing ListCards. Check the Depth header requirement in the RFC.
- This services batch body fetches (io-offline WantsFetch with a handle list) in one round-trip instead of one GET per card.

## 5. Etag-only card enumeration

Files: src/rfc6352/card/.

- ListCards hardcodes CARD_PROPS = [GETETAG, ADDRESS_DATA], so a full-spine enumerate downloads every body. Add an EnumCards coroutine (or a props parameter on ListCards) that queries GETETAG only and returns rows of a new CardRef type (id plus etag, no data).

## Conventions and checks

Follow the existing io-webdav style: one coroutine per file with a doc example, shared types in the subdomain's own module, no_std with the client feature gated. After each task run cargo fmt, clippy and test through the flake:

```sh
nix develop --command cargo fmt
nix develop --command cargo clippy --all-features
nix develop --command cargo test
```

Update CHANGELOG.md (keepachangelog format, past-tense bullets) and the README feature list as coroutines land.

## Landed

All five tasks shipped (2026-07-05): the multistatus parser reads the top-level sync-token and the response-level status rows, addressbooks carry sync_token and ctag, the sync-collection REPORT (SyncCollection / SyncDelta) and the addressbook-multiget batch fetch (MultigetCards) coroutines exist, and ETag-only enumeration (EnumCards returning CardRef rows) is in place.

The file layout this plan references (types.rs, utils.rs) was retired on 2026-07-16: each subdomain's shared types and vocabulary now live in its own sibling module (rfc4918.rs, rfc4791/calendar.rs, rfc6352/addressbook.rs, rfc6352/card.rs) with the coroutine submodules beside it, and every single-coroutine result type moved into its coroutine file, aligning io-webdav with the Pimalaya types-placement guideline (no types.rs catch-all, no doc-inlined re-export flatten). SYNC_TOKEN, GETCTAG and CALENDARSERVER landed as generic DAV vocabulary in rfc4918 rather than being hoisted from a calendar utils module.
