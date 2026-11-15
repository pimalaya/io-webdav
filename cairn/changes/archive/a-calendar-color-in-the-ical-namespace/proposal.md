---
cairn: change
id: a-calendar-color-in-the-ical-namespace
status: landed
created: 2026-11-15
---

# A calendar color in the iCal namespace

## Why

Line W5 of the CLI alignment round (.github/cairn/changes/cli-alignment-2026-11-15/tasks.md), from calendula's K24 (I-F3): io-webdav writes and reads a calendar's color as `calendar-color` in the inf-it namespace (`http://inf-it.com/ns/ab/`), the namespace of the CardDAV address book color. The calendar color clients and servers share is `calendar-color` in Apple's iCal namespace (`http://apple.com/ns/ical/`): iCloud ignores the inf-it one (`calendar create --color` dropped silently, `calendar update --color` failing after the rest applied), and iCloud, Fastmail, Nextcloud and Radicale all serve Apple's.

A server storing dead properties may hold both on one calendar: the inf-it one an earlier io-webdav wrote, the Apple one another client or the server itself set. The typed multistatus matched properties by local name alone, so the two were the same field, the first answered winning.

## What

- `rfc4791::calendar::CALENDAR_COLOR` names Apple's `calendar-color` (new `APPLE_ICAL` namespace, prefix `A`); create and update write it.
- New `INFIT_CALENDAR_COLOR`; a listing requests both, and reads Apple's, the inf-it one when Apple's is absent or empty.
- The typed multistatus resolves the namespace of each property (the declarations in scope, from the root down to the property element) and matches a struct field named in Clark notation, `{uri}local`, to the properties of that namespace only. `CalendarProps` names Apple's color that way, so the order a server answers them in no longer matters.
- The address book color stays `addressbook-color` in the inf-it namespace.

## Decisions

- **Namespaces at the property level only.** Properties are what WebDAV names by namespace; their values (component sets, report sets, hrefs) keep matching by local name. A namespace is resolved once per property, so the parse stays linear and a type naming no Clark field reads as before.
- **No removal of the inf-it color on update.** A PROPPATCH removing a property the server does not know may be refused, and a refused property fails the update: the stale inf-it value stays, and is read only when Apple's is absent.
- **`INFIT` kept** in `rfc4791::calendar`, so nothing importing it breaks.

## Scope / non-goals

- No version bump: the release is the owner's.
- calendula's colour read-back after a create (src/caldav/backend.rs `create_calendar`) needs no change: it reads the listing.
