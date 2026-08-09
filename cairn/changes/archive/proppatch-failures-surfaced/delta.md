---
cairn: change
id: proppatch-failures-surfaced
status: landed
created: 2026-08-09
---

# Delta

## ADDED Requirements

### Requirement: Refused properties are surfaced
A parsed response SHALL carry the properties a non-2xx propstat named, each with that propstat's status, alongside the properties it keeps from 2xx propstats. The PROPPATCH coroutine SHALL return the parsed multistatus, and the client SHALL fail when a property update comes back refused, rather than reporting success on a 207 that changed nothing.

#### Scenario: A property the server refuses
- GIVEN a PROPPATCH answered 207 with a 403 propstat
- WHEN the client runs it
- THEN it fails, naming the refused property and its status

### Requirement: An HTTP error is summarised, not dumped
A non-2xx status SHALL render as a summary of the body: its DAV responsedescription when it carries one, else the body with markup stripped, whitespace collapsed and length capped. An empty body SHALL render as the status alone. The raw body SHALL remain available on the error value for consumers that inspect it.

## MODIFIED Requirements

### Requirement: Multistatus parsing
The parser SHALL be vocabulary-agnostic, matching by local name and ignoring namespace prefixes. It SHALL keep properties from 2xx propstats only, recording the properties of non-2xx propstats as failures carrying their status, while still surfacing responses carrying no 2xx propstat as entries with empty props and their response-level status. It SHALL read the top-level sync-token. Predefined and numeric character references SHALL be resolved, unknown entity references kept verbatim, and malformed input SHALL yield whatever parsed before the error.

## REMOVED Requirements
