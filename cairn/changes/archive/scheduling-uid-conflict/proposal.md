---
cairn: change
id: scheduling-uid-conflict
status: landed
created: 2026-10-02
---

# A scheduling UID conflict is a refused duplicate UID

## Why

Fastmail refuses a second event under one UID with RFC 6638's `CALDAV:unique-scheduling-object-resource` precondition, wrapped in a 403, not with RFC 4791's `no-uid-conflict`. `is_duplicate_uid()` only knows the latter, so the refusal surfaces as a plain `HttpStatus` and the live Fastmail CalDAV flow fails (CI run of 51626e4). A caller branching on the duplicate signal, a sync engine keying a second copy apart, misses it the same way.

RFC 6638 makes a scheduling object resource's UID unique across every calendar of the user, so the refusal is the same fact on a wider scope: the UID is already held.

## What

- src/rfc4918/put.rs: `duplicate_uid` also matches `unique-scheduling-object-resource`, read as an element like `no-uid-conflict`, whatever the status.
- tests/rfc4791.rs: the create and update case gains Fastmail's 403 body.
- Spec (webdav-core, caldav), CHANGELOG `Fixed`.
