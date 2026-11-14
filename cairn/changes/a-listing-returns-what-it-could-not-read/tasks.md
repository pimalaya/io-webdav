---
cairn: tasks
change: a-listing-returns-what-it-could-not-read
---

# Tasks

- [ ] `WebdavUnreadRow` beside `WebdavResponseOf` (src/rfc4918/typed.rs), documented, re-exported nowhere (the crate exports nothing twice).
- [ ] `card_from_response` and `item_from_response` answer an entry, an unread row, or nothing.
- [ ] `CarddavCardListOk::unread`; `CaldavItemListOk { items, truncated, unread }`; the two multigets' outputs carry `unread`.
- [ ] src/client.rs: the std client's list and multiget methods return the new outputs.
- [ ] Tests: a card list and an item list over a multistatus holding a card or item, a self-entry, a 403 row, a row whose data propstat is 404, and a 507 row (truncated); a multiget naming a missing href.
- [ ] `nix develop --command cargo test --all-features`, clippy `--all-targets -- -D warnings` with `--all-features` and `--no-default-features`, `cargo fmt`.
- [ ] CHANGELOG; spec (carddav.md and caldav.md, see delta.md) folded; log cairn/log/<date>-a-listing-returns-what-it-could-not-read.md (frontmatter `cairn: log`, `change`, `landed:`); status landed.
- [ ] An agent runs no git write command; the owning session commits `feat!: return the rows a listing could not read` and pushes, so cardamum and calendula can pin it.
