---
cairn: log
change: scheduling-uid-conflict
landed: 2026-10-02
---

# A scheduling UID conflict is a refused duplicate UID

`duplicate_uid` (src/rfc4918/put.rs) now also matches RFC 6638's `unique-scheduling-object-resource` precondition, read as an element like `no-uid-conflict`, so `is_duplicate_uid()` holds for the refusal Fastmail answers a duplicate event with (403). The live Fastmail CalDAV flow failed on it in CI (run of 51626e4). tests/rfc4791.rs covers Fastmail's body offline.

Verified: offline suite green, clippy clean on all features. The live Fastmail flow was not rerun locally (the account address is not on this machine); the next CI run confirms it.

Capabilities moved: [webdav-core](../spec/webdav-core.md) MODIFIED "A refused duplicate UID is named"; [caldav](../spec/caldav.md) and [packaging](../spec/packaging.md) name the second precondition.
