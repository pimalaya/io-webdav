---
cairn: tasks
change: a-report-body-comes-back-whole
---

# Tasks

- [x] src/rfc4918/typed.rs: `whole_text` and its newtype name; `push_text` keeps every piece of a leaf, skipping blank pieces only between children.
- [x] src/rfc6352/card.rs and src/rfc4791/item.rs: `address_data` and `calendar_data` read whole; blank data stays unreadable.
- [x] Tests: tests/rfc6352.rs `a_card_keeps_its_address_data_whole` and tests/rfc4791.rs `an_item_keeps_its_calendar_data_whole` (list and multiget: a final escaped CRLF kept, indentation kept, a blank body unreadable), both failing with the whole read neutralised; tests/rfc4918.rs `a_parsed_property_keeps_its_whole_text_beside_the_trimmed_one`.
- [x] cargo-semver-checks 0.47.0 against v0.9.0 as a patch release: no finding.
- [x] fmt, `cargo test --all-features`, `cargo test --no-default-features`, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, `cargo deny check`.
- [x] CHANGELOG; spec (carddav.md, caldav.md, webdav-core.md, see delta.md) folded; log cairn/log/2026-11-15-a-report-body-comes-back-whole.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases 0.9.1.
