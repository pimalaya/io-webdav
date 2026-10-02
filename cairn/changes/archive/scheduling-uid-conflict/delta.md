---
cairn: delta
change: scheduling-uid-conflict
---

# Delta

## ADDED Requirements

## MODIFIED Requirements

### Requirement: A refused duplicate UID is named
A write answered with the CalDAV or CardDAV no-uid-conflict precondition, or with the CalDAV unique-scheduling-object-resource precondition (RFC 6638, the same refusal across every calendar of the user), SHALL surface as a dedicated error, distinct from any other send failure, carrying the status and the raw body like the others. The precondition element SHALL be what is matched, not the status, since the RFCs name the element and recommend the status.

## REMOVED Requirements
