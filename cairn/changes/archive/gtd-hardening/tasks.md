---
cairn: tasks
change: gtd-hardening
---

# Tasks

- [x] Calendar and address book ids: the last segment for a direct child of the home set, the href's path otherwise; ids starting with a slash address that path (calendar and address book `join_path`, the client's `calendar_path` and `addressbook_path`)
- [x] Reconnect once on a connection the server closed before answering: `connect` records how, `set_reconnect` for a caller's stream, `set_stream` drops it
- [x] `CaldavItemMove` and `WebdavClientStd::move_item`, the duplicate-UID refusal mapped
- [x] Offline tests for each (regression tests failing on the old code first); `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test --all-features`
- [x] Live: Radicale, Fastmail (move between two scratch calendars), Google (a secondary scratch calendar's id and an event written into it)
- [x] CHANGELOG; fold into spec/caldav.md, spec/carddav.md, spec/client.md, spec/webdav-core.md; log entry

## Notes
- Provisional (identity): the home set and the href are compared percent-decoded, the id keeps the href's own encoding; a relative href keeps its last segment, as before. An existing store keyed on an old `events` id of a Google secondary calendar sees that calendar under its new id, which every such store needed anyway: all of them collapsed into one.
- Provisional (reconnect): only a request whose response had not begun is resent, and only once; BrokenPipe, ConnectionAborted, ConnectionReset, NotConnected, UnexpectedEof and a read of zero bytes count as closed. A request the server may have applied before closing without a byte of answer is resent too, as RFC 9112 §9.3.1.1 allows for idempotent methods; every request the crate sends is one (a create carries `If-None-Match: *`, so a replay is refused rather than doubled).
- Provisional (move): the `Destination` is an absolute URL built from the base URL; `MOVE` between two accounts or hosts is out.
- Live, 2026-10-29: Radicale's built-in server (HTTP/1.0, closing after every response) passed caldav, carddav and caldav_move on one client from `connect`, no `set_stream` between requests. Fastmail passed caldav and caldav_move (the move a create-then-delete cannot do there). Google, on two scratch calendars made and deleted through the Calendar API: each secondary calendar listed as `/caldav/v2/<calendar>%40group.calendar.google.com/events`, an event written into one and read back from it, both stating the component set `{VEVENT}` only; `MOVE` answered 404 (Google's HTML "The requested URL ... was not found on this server") with a path or an absolute `Destination` alike, so Google has no `MOVE`.
