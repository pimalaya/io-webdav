---
cairn: tasks
change: enumeration-without-sync-collection
---

# Tasks

- [x] Add `UnsupportedReport`, raised on the `DAV:supported-report` precondition
      and on `405` and `501`.
- [x] Parse `supported-report-set` and return it on the collection listings.
- [x] Cache it beside the principal and home-set discovery.
- [x] Add a `PROPFIND` card enumeration at Depth 1 on `WebdavPropfind`, which
      parses nothing and so survives an unparseable member. *(Protocol-neutral,
      so it serves calendars too: it lives in the enumeration coroutine beside
      the report rather than in the CardDAV layer.)*
- [x] Add the caller-fed enumeration flag to the client options, one entry point
      per collection type, both paths implemented.
- [x] Set the truncated flag from a 507 row on the query path.
- [x] Cover the precondition apart from a permission refusal, a credential
      failure and a server fault, using the body a real server answers with.
- [x] Cover a truncated listing, and a collection holding an unparseable member.
- [x] CHANGELOG.md.
- [x] Fold the delta into cairn/spec/{sync,carddav,caldav,client}.md and log it.
