---
cairn: tasks
change: an-enumeration-returns-what-it-could-not-read
---

# Tasks

- [x] `WebdavSyncDelta::unreadable: Vec<WebdavUnreadableEntry>` (src/rfc6578/sync_collection.rs), documented; `WebdavUnreadableEntry`'s docs name the enumeration too (src/rfc4918/typed.rs).
- [x] `from_multistatus`: the self-entry before the error statuses, a non-2xx row other than 404 and 507 unreadable, an id-less one skipped; the only caller pair (the REPORT and `PROPFIND` arms of `WebdavSyncCollection::resume`) and the only builder of a `WebdavSyncDelta`.
- [x] Tests, failing first: `an_enumeration_returns_the_member_rows_it_could_not_read` (both enumerations: a 403 self-entry, a member, a 403 row, a 503 row, a refused `getetag` under a 2xx, a 404, a bare 507) and `initial_sync_sorts_the_delta_rows` now asserting its 500 row unreadable; both failed with the field added and left empty.
- [x] `nix develop --command cargo fmt --check`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo test --all-features`.
- [x] CHANGELOG; spec sync.md folded (see delta.md); log cairn/log/2026-11-21-an-enumeration-returns-what-it-could-not-read.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases, then neverest pins the release.
