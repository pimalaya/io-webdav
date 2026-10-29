---
cairn: log
change: gtd-hardening
landed: 2026-10-29
---

# Calendars addressed by their path, a closed connection resent, an item moved

Three findings of unfog's live provider runs (unfog cairn/changes/gtd-hardening, track E), each wrong for every server.

A listed calendar or address book outside the home set now carries its href's path as its id, starting with a slash, and such an id addresses that path (`rfc4918::collection_id`, `collection_path`; the calendar and address book `join_path`, the client's `calendar_path` and `addressbook_path`). A collection directly under the home set keeps its last segment, so no id changed on a server that already worked. Google lists every calendar as `/caldav/v2/<calendar>/events/`: every calendar was named `events`, the listing kept one of them, and every verb reached the primary calendar. A breaking change for a consumer keyed on those ids.

A client given a way to reconnect resends a request, once, when the server closed the connection before a byte of its answer arrived; `connect` gives it the URL and options it connected with, `set_reconnect` gives one to a client on a caller's stream, `set_stream` drops it so a client moved to another host never reconnects to the old one. Radicale answers `Connection: close` after a PUT, and its built-in server after every response.

`CaldavItemMove` and `WebdavClientStd::move_item` move an item into another calendar of the same server with `MOVE`, `Overwrite: F`, an absolute `Destination` and an optional `If-Match`, keeping its id; a refused `UID` is `DuplicateUid`.

Capabilities moved: caldav (Calendar identity, Item verbs, Item move), carddav (Address book identity), client (Closed connection), webdav-core (Resource identity).

Verified: offline suites 215 tests, green on all features, clippy clean; three of the new tests, run against 62fad89's code, failed (`Send(Send(Eof))` on the request after a `Connection: close`, and two Google calendars collapsing into one `events`, CardDAV alike). Live on 2026-10-29: Radicale's built-in server passed caldav, carddav and caldav_move on one connected client; Fastmail passed caldav and caldav_move; Google, on two scratch calendars made and deleted through the Calendar API, listed each by its path, took an event into a secondary calendar and read it back there, stated `{VEVENT}` as the component set, and answered `MOVE` with 404: Google has no `MOVE`.
