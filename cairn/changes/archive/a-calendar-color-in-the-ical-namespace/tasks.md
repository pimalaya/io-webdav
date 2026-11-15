---
cairn: tasks
change: a-calendar-color-in-the-ical-namespace
---

# Tasks

- [x] src/rfc4918/xml.rs: a start tag carries its prefix.
- [x] src/rfc4918/typed.rs: the envelope keeps the namespace declarations in scope down to the property level, each property node its namespace URI; a struct field named `{uri}local` matches the properties of that namespace only; the module header says so.
- [x] src/rfc4791/calendar.rs: `APPLE_ICAL`, `CALENDAR_COLOR` in it, `INFIT_CALENDAR_COLOR`, both in `LIST_PROPS`; src/rfc4791/calendar/list.rs reads Apple's, then the inf-it one.
- [x] Tests: src/rfc4918/typed.rs `parse_multistatus_as_reads_a_clark_field_from_its_namespace_only` (root, response and inline declarations, scopes closed with their element); tests/rfc4791.rs `a_calendar_color_is_written_in_the_apple_ical_namespace` (MKCALENDAR and PROPPATCH) and `a_calendar_color_is_read_from_either_namespace_the_apple_one_first`. Each fails with `CALENDAR_COLOR` back in the inf-it namespace or the Clark match disabled.
- [x] fmt, `cargo test --all-features`, `cargo test --no-default-features`, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, `missing_docs` denied, `cargo deny check`.
- [x] CHANGELOG; spec (caldav.md, webdav-core.md, see delta.md) folded; log cairn/log/2026-11-15-a-calendar-color-in-the-ical-namespace.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases.
