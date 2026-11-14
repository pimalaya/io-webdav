---
cairn: tasks
change: a-listing-returns-what-it-could-not-read
---

# Tasks

- [x] `WebdavUnreadableEntry` (planned as `WebdavUnreadRow`) beside `WebdavResponseOf` (src/rfc4918/typed.rs), documented, re-exported nowhere (the crate exports nothing twice).
- [x] `card_from_response` and `item_from_response` answer an entry, an unreadable entry, or nothing.
- [x] `CarddavCardListOk::unreadable`; `CaldavItemListOk { items, truncated, unreadable }`; `CarddavCardMultigetOk { cards, unreadable }` and `CaldavItemMultigetOk { items, unreadable }`.
- [x] src/client.rs: the std client's list and multiget methods return the new outputs.
- [x] Tests: a card list and an item list over a multistatus holding a card or item, a self-entry, a 403 row, a row whose data propstat is 404, and a 507 row (truncated); a multiget naming a missing href.
- [x] `nix develop --command cargo test --all-features`, clippy `--all-targets -- -D warnings` with `--all-features` and `--no-default-features`, `cargo fmt`. The `--no-default-features --all-targets` run fails on the test targets before this change too (tests/common/mod.rs and tests/google.rs import `io_webdav::client` ungated); `--no-default-features` without `--all-targets` is clean.
- [x] CHANGELOG; spec (carddav.md and caldav.md, see delta.md) folded; log cairn/log/<date>-a-listing-returns-what-it-could-not-read.md (frontmatter `cairn: log`, `change`, `landed:`); status landed.
- [ ] An agent runs no git write command; the owning session commits `feat!: return the rows a listing could not read` and pushes, so cardamum and calendula can pin it.
