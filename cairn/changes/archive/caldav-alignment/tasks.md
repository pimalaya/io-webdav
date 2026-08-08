---
cairn: tasks
change: caldav-alignment
---

- [x] Refresh the dependency lockfile
- [x] Make item ids verbatim: stop appending .ics on join, stop stripping it on list
- [x] Honour the Location header on item create, sharing the helper with cards
- [x] Hoist ItemEntry, add ItemRef, share the property selector and the entry mapper
- [x] Guard the item listing against the collection self-entry
- [x] Add the sync token to Calendar and to the listing properties
- [x] Add the component set to Calendar, read and written
- [x] Teach property sets to carry raw markup as well as text
- [x] Teach the multistatus parser to read child name attributes
- [x] Write the calendar time zone, which the listing already read
- [x] Add the calendar-multiget body helper and the MultigetItems coroutine
- [x] Add the EnumItems coroutine
- [x] Add enum_items, multiget_items and sync_items to the client
- [x] Transpose the card test suite, including the listed-id round-trip regression
- [x] Extend the live CalDAV flows with the sync read-side
- [x] Restore 100% coverage on both protocols
