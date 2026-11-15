---
cairn: log
change: a-calendar-color-in-the-ical-namespace
landed: 2026-11-15
---

# A calendar color in the iCal namespace

A calendar's color is written as Apple's `calendar-color` (`http://apple.com/ns/ical/`) and read from it first, the inf-it one (`http://inf-it.com/ns/ab/`) only when it is absent (line W5 of the CLI alignment round, from calendula's K24, I-F3).

**The defect.** `rfc4791::calendar::CALENDAR_COLOR` named the inf-it property, the namespace of the CardDAV address book color. iCloud ignores it: live testing on 2026-11-15 saw `calendula calendar create --color` drop the color silently and `calendar update --color` fail after the name and description applied. iCloud, Fastmail, Nextcloud and Radicale serve the Apple property.

**The fix.** `CALENDAR_COLOR` is the Apple property (new namespace `APPLE_ICAL`), so create and update write it; `INFIT_CALENDAR_COLOR` names the old one, and the listing requests both. Reading "Apple's first" needed namespaces: the typed multistatus matched properties by local name alone, so the two `calendar-color` were one field, the first the server answered winning. The scanner now hands each start tag's prefix, the envelope keeps the namespace declarations in scope down to the property level (root, response, propstat, prop and the property itself, each closed with its element), each property node carries its namespace URI, and a struct field named in Clark notation (`{uri}local`) matches the properties of that namespace only. A type naming no Clark field reads as before. The address book color is unchanged.

**Capabilities moved.** caldav: "Calendar color" added. webdav-core: "Typed multistatus" modified (Clark-notation fields).

**Verified.** `parse_multistatus_as_reads_a_clark_field_from_its_namespace_only` (src/rfc4918/typed.rs), `a_calendar_color_is_written_in_the_apple_ical_namespace` and `a_calendar_color_is_read_from_either_namespace_the_apple_one_first` (tests/rfc4791.rs) fail with `CALENDAR_COLOR` back in the inf-it namespace or the Clark match disabled, and pass with them. fmt, tests with all and with no default features, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, `missing_docs` denied and `cargo deny check` are clean. No live suite ran.

**Semver.** New items only (`APPLE_ICAL`, `INFIT_CALENDAR_COLOR`); `CALENDAR_COLOR` keeps its type and changes the namespace it names, which callers sending it (calendula's hand-run PROPPATCH) follow without a change.
