---
cairn: delta
change: a-caller-coroutine-is-resent
---

# Delta

## ADDED Requirements

None.

## MODIFIED Requirements

### Requirement: Light client
(client) The client SHALL wrap a single stream the caller opened, any blocking reader and writer, and expose one method per WebDAV operation. It SHALL also expose the stream, so a higher-level crate can pump its own coroutines against the same connection while reusing the discovery cache, and SHALL run a coroutine the caller builds through the exchange its own methods use, Closed connection included.

#### Scenario: A raw REPORT after the server closed
- GIVEN a client given a way to reconnect, whose server closed the connection after the previous response
- WHEN the caller runs a REPORT coroutine of its own through the client
- THEN the REPORT reaches the server on a fresh connection and its parsed multistatus is returned

## REMOVED Requirements

None.
