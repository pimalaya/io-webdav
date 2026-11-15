---
cairn: log
change: a-calendar-update-removes
landed: 2026-11-15
---

# A calendar update removes

The calendar update takes a `CaldavCalendarPatch`, each property left alone, removed or set, all in one PROPPATCH, as the address book update has since 2026-08-09 (line W6 of the CLI alignment round, from calendula's K21).

**The gap.** `CaldavCalendarUpdate` and `WebdavClientStd::update_calendar` took a flat `CaldavCalendar` and set what it carried, so a cleared description or color could not be sent. calendula pumped its own PROPPATCH over the client's stream to remove them, bypassing the client's reconnect on a closed connection.

**The change.** `rfc4791::calendar::CaldavCalendarPatch` (`id`, `display_name`, `description`, `color`, `tz`, each `Option<Option<String>>`) and `property_updates`, the twins of the CardDAV ones; `CaldavCalendarUpdate::new` and `update_calendar` take the patch, and the client verifies a removed property as it verifies a set one. The component set is no part of the patch: RFC 4791 section 5.2.3 protects it once the calendar exists, and the old update sent it whenever non-empty.

**Capabilities moved.** caldav: "Calendar update" added; "Calendar properties" modified (the component set is written on create only).

**Verified.** `update_calendar_removes_the_properties_the_patch_clears` (tests/rfc4791.rs) and the calendar half of `a_property_the_server_never_mentions_fails_the_update` (tests/client.rs) fail with the removals dropped, and pass with them; the other calendar update tests and the live suite's rename (tests/common/live.rs, compiled, not run) moved to the patch. Checks as for a-calendar-color-in-the-ical-namespace, all clean.

**Semver.** Breaking: the parameter type of `CaldavCalendarUpdate::new` and `WebdavClientStd::update_calendar` changed. cargo-semver-checks 0.47.0 against 0.9.1 (crates.io) passes all 222 checks, having no lint for a changed parameter type.

**Follow-ups, not done here.** calendula src/caldav/backend.rs `update_calendar` and `patch` build the patch and call `update_calendar`, dropping src/caldav/client.rs `proppatch` and `accepted`; neverest src/dav/client.rs `rename_collection` (its CalDAV arm) builds a `CaldavCalendarPatch`.
