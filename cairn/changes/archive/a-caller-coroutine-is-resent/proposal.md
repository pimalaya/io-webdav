---
cairn: change
id: a-caller-coroutine-is-resent
status: landed
created: 2026-11-15
---

# A caller's coroutine is resent

## Why

Line K27 of the CLI alignment round (.github/cairn/changes/cli-alignment-2026-11-15/tasks.md), from W8. calendula sends two requests by hand over `WebdavClientStd::stream`: the REPORT of `caldav report raw` (src/caldav/report/raw.rs) and the PROPFIND of `account check` on a pinned home (src/account/check.rs). cardamum does the same in src/carddav/report/raw.rs and `propfind_home` (src/carddav/client.rs), neverest in its own `run` (src/dav/client.rs). A stream pumped by hand misses what every client method gets: when the server closed the connection after the previous request, the request is resent once on a fresh connection.

## What

- `WebdavClientStd::run`, the pump every client method goes through, is public: it runs any coroutine yielding `WebdavYield` over the client stream until it completes, with the resend.
- The `stream` field and the module header point at it.

## Decisions

- **The pump, not a raw request method.** The line suggested a method taking a method, a path, a depth and a body and answering a status and a body. The callers already build a `WebdavReport` or a `WebdavPropfind` and use its parsed multistatus and its typed `WebdavSendError`, which their error codes are read from; a raw status and body would have each of them parse and map again. Making `run` public adds one method and no type, takes those coroutines unchanged, and covers every coroutine a caller pumps by hand.
- **Redirect-aware coroutines stay inside.** Discovery is the client's own; `run_redirect` stays private.

## Scope / non-goals

- No version bump. A public method added is a minor change on its own; it ships with a-served-href-beside-the-id, which breaks.
- No consumer changes here.
