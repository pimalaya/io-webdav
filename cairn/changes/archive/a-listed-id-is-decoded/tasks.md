---
cairn: tasks
change: a-listed-id-is-decoded
---

# Tasks

- [x] src/rfc4918.rs: `segment_id`; `collection_id`, `id_from_location` and `WebdavResponseEntry::id` read through it; `encode_id`, `collection_path` and `sent_id` docs.
- [x] src/rfc4918/typed.rs: `WebdavResponseOf::id` reads through it; `WebdavUnreadableEntry::id` doc.
- [x] src/rfc4791/item.rs, src/rfc6352/card.rs and their coroutines: ids taken owned from the `Cow`; the id docs of every ref, entry, verb and returned id; src/lib.rs header.
- [x] Tests: tests/rfc4918.rs `a_listed_segment_is_decoded_when_its_encoding_gives_it_back` (decoded and kept segments, `collection_id`, `id_from_location`, the id addressing its segment); tests/rfc4791.rs `a_listed_calendar_or_item_id_is_decoded_when_its_encoding_gives_it_back`; tests/rfc6352.rs `a_listed_card_id_is_decoded_when_its_encoding_gives_it_back` (multiget, unreadable rows, enumeration); tests/client.rs `an_id_round_trips_through_create_list_and_address`. The 0.9.1 tests pinning the encoded returned id (`create_and_update_item_return_the_segment_sent`, `move_item_returns_the_segment_sent`, `create_and_update_card_return_the_segment_sent`) now expect the decoded id, renamed `*_return(s)_the_id_a_listing_gives`. All fail with `segment_id` returning the segment as it stands.
- [x] fmt, `cargo test --all-features`, `cargo test --no-default-features`, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, `missing_docs` denied, `cargo deny check`.
- [x] CHANGELOG; spec (webdav-core.md, caldav.md, carddav.md, see delta.md) folded; log cairn/log/2026-11-15-a-listed-id-is-decoded.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases.
