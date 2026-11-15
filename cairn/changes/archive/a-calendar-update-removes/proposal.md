---
cairn: change
id: a-calendar-update-removes
status: landed
created: 2026-11-15
---

# A calendar update removes

## Why

Line W6 of the CLI alignment round (.github/cairn/changes/cli-alignment-2026-11-15/tasks.md), from calendula's K21: `CaldavCalendarUpdate` and `WebdavClientStd::update_calendar` took a flat `CaldavCalendar` and only set properties, so a cleared description or color could not be sent. calendula pumps its own PROPPATCH over the client's stream to remove them (its src/caldav/client.rs `proppatch`, called from src/caldav/backend.rs `patch`), which bypasses the client's reconnect on a closed connection. The CardDAV twin has taken a doubly optional patch since 2026-08-09 (`CarddavAddressbookPatch`).

## What

- `rfc4791::calendar::CaldavCalendarPatch`: `id`, then `display_name`, `description`, `color` and `tz` as `Option<Option<String>>` (`None` leaves the property alone, `Some(None)` removes it, `Some(Some(value))` sets it).
- `rfc4791::calendar::property_updates` splits a patch into the PROPPATCH set pairs and the properties to remove; `property_set` is the MKCALENDAR side only.
- `CaldavCalendarUpdate::new` and `WebdavClientStd::update_calendar` take the patch, and send one PROPPATCH; the client verifies a removed property as it verifies a set one.

## Decisions

- **Mirror the CardDAV API**, names and shape, so the twins read alike; the old signature goes (0.x, breaking).
- **No component set in the patch.** RFC 4791 section 5.2.3 makes `supported-calendar-component-set` protected; a PROPPATCH carrying it is refused. The old update sent a non-empty set; none of calendula, neverest or carillon did.
- **The time zone joins the patch**, settable and removable like the rest, the listing reading it.

## Scope / non-goals

- No version bump: the release is the owner's.
- calendula is not touched: it drops its hand-run PROPPATCH when it moves to the release (src/caldav/backend.rs `update_calendar` and `patch`, src/caldav/client.rs `proppatch` and `accepted`).
