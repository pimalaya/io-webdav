---
cairn: change
change: duplicate-link-id-mints-an-item
---

# Delta

## ADDED Requirements

### Requirement: A refused duplicate UID is named
A write answered with the CalDAV or CardDAV no-uid-conflict precondition SHALL surface as a dedicated error, distinct from any other send failure, carrying the status and the raw body like the others. The precondition element SHALL be what is matched, not the status, since the RFCs name the element and recommend the status.

The refusal is the server saying the collection already holds this UID, which is a state a consumer acts on: it names the resource to fix and the collection to fix it in. Left as an opaque conflict it is indistinguishable from a quota or a lock, and a consumer can only report a number.

#### Scenario: A duplicate UID is refused
- GIVEN a collection already holding a resource with a UID
- WHEN a client PUTs another resource carrying that UID
- AND the server answers with the no-uid-conflict precondition
- THEN the client receives the duplicate-uid error rather than an opaque status

## MODIFIED Requirements

### Requirement: Preconditions
Creation SHALL send If-None-Match with a star so the server rejects the write when the resource already exists. Update and delete SHALL accept an optional If-Match so the caller can gate the write on the last-known ETag. A write refused with the CALDAV:no-uid-conflict precondition SHALL surface as the dedicated duplicate-uid error, which is a different refusal from a resource name already taken and keeps its own signal.

### Requirement: Preconditions (carddav.md)
Creation SHALL send If-None-Match with a star. Update and delete SHALL accept an optional If-Match. A write refused with the CARDDAV:no-uid-conflict precondition SHALL surface as the dedicated duplicate-uid error, which is a different refusal from a resource name already taken and keeps its own signal.

> Folding note: the requirement is named `Preconditions` in both caldav.md and carddav.md. Fold the block above into carddav.md and the one before it into caldav.md, dropping the parenthesised file name from the heading.

## REMOVED Requirements
