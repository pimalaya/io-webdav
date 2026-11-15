---
cairn: tasks
change: a-calendar-update-removes
---

# Tasks

- [x] src/rfc4791/calendar.rs: `CaldavCalendarPatch`, `property_updates`; `property_set` documented as the MKCALENDAR side.
- [x] src/rfc4791/calendar/update.rs: `CaldavCalendarUpdate::new` takes the patch and sends its removals; the example clears a description.
- [x] src/client.rs: `update_calendar` takes the patch.
- [x] Tests: tests/rfc4791.rs `update_calendar_sends_proppatch` and `update_calendar_removes_the_properties_the_patch_clears` (the twins of the CardDAV ones), tests/client.rs `a_property_the_server_never_mentions_fails_the_update` (a removed color and description the server never names fail the update), the other calendar update call sites and the live suite (tests/common/live.rs) on the patch. The removal test and the client one fail with the removals dropped.
- [x] fmt, `cargo test --all-features`, `cargo test --no-default-features`, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, `missing_docs` denied, `cargo deny check`.
- [x] CHANGELOG; spec (caldav.md, see delta.md) folded; log cairn/log/2026-11-15-a-calendar-update-removes.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases.
