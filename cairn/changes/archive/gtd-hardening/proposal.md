---
cairn: change
id: gtd-hardening
status: landed
created: 2026-10-29
---

# Calendars addressed by their path, a closed connection resent, an item moved

> Cross-repo change, same id in pimdir, io-pimdir, io-webdav (here), neverest, unfog and android. This crate's part is independent of the storage work and lands first: the others pin the revision it pushes.

## Why

Three findings of the live provider runs of 2026-10-29 (unfog cairn/changes/gtd-hardening, track E) land here, each one wrong for every server, not only the one that showed it.

- **Calendar identity.** A listed calendar's id is the last segment of its href and every verb rebuilds the path as the home set joined with that id. RFC 4791 treats a collection URL as opaque, and nothing makes a calendar a direct child of the home set. Google lists every calendar as `/caldav/v2/<calendar>/events/`: every id comes out `events`, and every verb addresses `<home>/events`, the primary calendar, whichever calendar the caller meant. A client writing a task into a secondary calendar writes it into the primary one.
- **Closed connection.** The client keeps one stream for its life. A server may close it after any response (Radicale answers `Connection: close` after a PUT; an idle keep-alive times out everywhere), and the next request then fails with an I/O error or an unexpected end of stream, before the server read a byte of it. Each consumer worked around it (unfog reconnects and retries once); the client that opened the connection itself knows how to open another one.
- **Item move.** Fastmail keeps one UID per account: an item moved between two of its calendars by a create then a delete is refused, 403 "WebDAV collection already holds a resource with the same UID", since the source still holds it. A WebDAV `MOVE` moves the resource itself, so the UID never exists twice. The crate has a generic `MOVE` coroutine but no calendar item verb over it.

## What

- A listed calendar or address book whose collection is a direct child of the home set keeps today's id, the last segment of its href, so no id changes on a server that already works. Any other collection's id is its href's path, starting with a slash, as the server served it. An id starting with a slash addresses that path; any other id addresses the home set joined with it.
- A client given a way to reconnect, which `connect` always gives, opens a fresh connection and resends the request in flight, once, when the server closed the connection before answering it. `set_reconnect` gives a client built on a caller's stream the same; `set_stream` drops it, so a stream swapped after a redirect never reconnects to the old host.
- `CaldavItemMove` and `WebdavClientStd::move_item` move a calendar item into another calendar of the same server with `MOVE`, `Overwrite: F`, and `If-Match` when the caller names an ETag. A refusal naming `no-uid-conflict` or `unique-scheduling-object-resource` is the duplicate-UID error, as for a create.

## Out

- A collection on another host than the client's base URL: its path is kept, but the client's single stream cannot reach it. No provider tested lists one.
- A CardDAV card move: no consumer needs one.
- No version bump: the consumers pin the git revision until the tests of this batch are done.
