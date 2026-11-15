---
cairn: tasks
change: a-served-href-beside-the-id
---

# Tasks

- [x] src/rfc4791/item.rs, src/rfc6352/card.rs: `href` on the refs and entries; `item_from_response`, `card_from_response` and both enumerations fill it.
- [x] src/rfc4791/calendar.rs, src/rfc6352/addressbook.rs: `href` on `CaldavCalendar` and `CarddavAddressbook` (`#[serde(default)]`); both listings fill it.
- [x] src/rfc4918.rs: `sent_id` becomes `sent_href`, the path of the URL sent; item create, update and move and card create and update keep that href (the `Location` when it names the resource, the `Destination` for a move) and read their id from it.
- [x] src/rfc4918/typed.rs: `WebdavUnreadableEntry::href` doc; src/lib.rs header.
- [x] Tests: tests/rfc4791.rs `a_listed_calendar_or_item_carries_its_href_as_served` (calendar listing, item listing and multiget with their unreadable rows, enumeration) and `a_written_item_carries_the_href_it_was_sent_to_or_located_at` (create with each `Location` spelling and without one, update, move); tests/rfc6352.rs `a_listed_addressbook_or_card_carries_its_href_as_served` and `a_written_card_carries_the_href_it_was_sent_to_or_located_at`; each over a `%20` segment, a raw space, raw non-ASCII and a `%2F` (tests/common/mod.rs `served_responses`, `served_pairs`); the two relocation tests check the absolute `Location` kept. Against the 0.10.0 sources they fail to compile, no `href` field on any of the eleven types.
- [x] fmt, `cargo test --all-features`, `cargo test --no-default-features`, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, `missing_docs` denied, `cargo deny check`.
- [x] CHANGELOG; spec (webdav-core.md, caldav.md, carddav.md, see delta.md) folded; log cairn/log/2026-11-15-a-served-href-beside-the-id.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases.
