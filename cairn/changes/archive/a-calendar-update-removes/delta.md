---
cairn: delta
change: a-calendar-update-removes
---

# Delta

## ADDED Requirements

### Requirement: Calendar update
(caldav) A calendar update SHALL take a partial update in which each of the display name, description, color and default time zone is left alone, removed (`DAV:remove`, RFC 4918 section 9.2) or set (`DAV:set`), all sent in one PROPPATCH. A removed property SHALL be verified as a set one is (A property update is verified, not assumed). The component set SHALL be no part of it, RFC 4791 section 5.2.3 protecting it once the calendar exists.

#### Scenario: A cleared description
- GIVEN a calendar with a name, a description and a color
- WHEN the caller updates it, setting the name and removing the description
- THEN one PROPPATCH sets the name and removes the description, and the color appears in neither

## MODIFIED Requirements

### Requirement: Calendar properties
(caldav) A listed calendar SHALL carry its id, display name, description, color, component set, ctag, sync token, default time zone and the reports the server advertises for it. Every property the listing reads SHALL also be writable on create, the advertised report set excepted, which RFC 3253 section 3.1.5 protects, so a calendar round-trips through create; an update SHALL write all of them but the component set (Calendar update).

## REMOVED Requirements

None.
