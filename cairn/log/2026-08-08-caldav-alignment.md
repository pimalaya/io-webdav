---
cairn: log
change: caldav-alignment
landed: 2026-08-08
---

# CalDAV alignment

Brought the CalDAV layer level with CardDAV, which had been a month ahead since the read-side sync work and the verbatim-id fix both landed for cards only. CardDAV was the reference throughout, transposed rather than reinterpreted.

Item ids became verbatim, curing the calendar twin of the .vcf bug: a listing stripped .ics while every addressing verb appended it, so any id not ending in .ics addressed nothing. Creation now honours the Location header, and the helper doing so moved into the RFC 4918 module rather than being duplicated per protocol. The item types, the property selector and the entry mapper were hoisted next to the coroutines the way cards already had them, which also gave the item listing the collection self-entry guard it was missing.

Two coroutines were added, ETag-only enumeration and a calendar-multiget batch fetch, alongside the client methods for both plus a sync-collection method for calendars, the report itself already being protocol-neutral.

Reading and writing supported-calendar-component-set forced two changes on the shared layer, since the property's value is attribute-carrying children rather than text: a parsed property now exposes each child's name attribute, and a property set pair now carries either escaped text or a raw markup fragment. While writing the property set, the calendar time zone was found to be read by the listing but never written, so it was silently dropped on create and update.

The dependency lockfile was refreshed in the same pass. Coverage returned to 100%, with the self-entry and empty-href guards now exercised on both protocols rather than only described.

Downstream this is breaking. Item ids take the full resource name, ItemEntry moved up next to ItemRef, property sets take a value type, and parsed property children are structs. pimalaya-linux can drop the extension shim it carried in its CalDAV sync adapter.

Spec updated: caldav (ADDED: enumeration, batch fetch, self-entry filtering, component set; MODIFIED: item identity, item verbs, calendar properties), webdav-core (ADDED: property values, property children), sync (ADDED: checkpoint properties).
