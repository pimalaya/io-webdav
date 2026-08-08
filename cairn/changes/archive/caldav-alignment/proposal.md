---
cairn: change
id: caldav-alignment
status: landed
created: 2026-08-08
---

# Bring CalDAV level with CardDAV

## Why

The read-side sync work was written for a CardDAV consumer and landed for cards only. The later verbatim-id fix also touched cards only. So the CalDAV layer sat a month behind its twin: no delta, no batch fetch, no ETag-only enumeration, and the same extension bug the cards had been cured of.

That last one had already reached a consumer. pimalaya-linux carried a shim putting .ics back on every id, with a comment pointing here.

## What

Transpose the CardDAV layer onto CalDAV rather than design anything new: same shapes, same names, same doc structure.

- Item ids become the href's last segment used verbatim, with the Location header honoured on create.
- Hoist the item types, the property selector and the entry mapper next to the coroutines, as cards already do.
- Add ETag-only enumeration and a calendar-multiget batch fetch.
- Give calendars the sync token their address book twin already carries, and expose sync-collection for them.
- Read and write supported-calendar-component-set, and stop dropping the time zone on write.
