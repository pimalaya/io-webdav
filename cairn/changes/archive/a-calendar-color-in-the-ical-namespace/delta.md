---
cairn: delta
change: a-calendar-color-in-the-ical-namespace
---

# Delta

## ADDED Requirements

### Requirement: Calendar color
(caldav) A calendar's color SHALL be written as `calendar-color` in the `http://apple.com/ns/ical/` namespace, by a create and an update alike. A listing SHALL request that property and `calendar-color` in the `http://inf-it.com/ns/ab/` namespace, and SHALL read the first, the second when the first is absent or empty, whatever order the server answers them in. An address book's color SHALL stay `addressbook-color` in the inf-it namespace.

#### Scenario: A calendar carrying both colors
- GIVEN a calendar holding an inf-it `calendar-color` and an Apple one, the server answering the inf-it one first
- WHEN the calendars are listed
- THEN the calendar's color is the Apple one

## MODIFIED Requirements

### Requirement: Typed multistatus
(webdav-core) The crate SHALL parse a multistatus into responses whose 2xx properties deserialize, merged across propstats, into a serde type the caller supplies. Children SHALL match by local name, prefixes ignored, the first of same-named properties winning. A struct field named in Clark notation, `{uri}local`, SHALL match only the properties of that namespace, each property's prefix resolved through the namespace declarations in scope, and a field named `local` alone the other same-named properties. Text SHALL read trimmed, borrowed from the body where it needs no decoding; attributes SHALL read as `@name` keys; consecutive same-named children SHALL fill a sequence; an empty element SHALL read as unit. The envelope (href, response status, refused properties, sync token, depth cap, whole-document rule) SHALL keep the semantics of Multistatus parsing. A response whose properties do not fit the type SHALL fail the parse, naming its href, rather than drop out: a listing missing a member reads as its removal.

#### Scenario: A property the type cannot hold
- GIVEN a typed multistatus expecting a number in `getcontentlength`
- WHEN a response carries text there
- THEN the parse fails naming that response's href

#### Scenario: Two properties sharing a local name
- GIVEN a response carrying `calendar-color` in two namespaces, both prefixes declared on the root
- WHEN it is parsed into a type naming one of them `{http://apple.com/ns/ical/}calendar-color` and the other `calendar-color`
- THEN each field reads the property of its own namespace, whichever comes first

## REMOVED Requirements

None.
