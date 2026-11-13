---
cairn: spec
capability: discovery
status: current
---

# Discovery

Resolving where a user's collections live, starting from the DAV context root and ending at a per-protocol home set. The context root itself is resolved upstream, by io-pim-discovery running RFC 6764.

### Requirement: Current user principal
The crate SHALL discover the current user principal URL from the DAV context root via the current-user-principal property (RFC 5397).

### Requirement: Home set resolution
The per-protocol home set SHALL be discovered from the principal URL, calendar-home-set for CalDAV and addressbook-home-set for CardDAV. Each SHALL yield nothing rather than fail when the multistatus carries no matching href. The property holds any number of hrefs (RFC 4791 section 6.2.1, RFC 6352 section 7.1.1): each SHALL be read as its own element, and the first non-empty one SHALL be the home set returned.

#### Scenario: A principal with two home sets
- GIVEN a principal whose home set property names two hrefs
- WHEN the home set is discovered
- THEN the first href is returned, resolved against the base URL

### Requirement: Redirects surfaced
Discovery coroutines SHALL surface a 3xx response to the caller instead of following it, reporting the target URL, whether the connection can be kept alive, and whether the target is same-origin. Providers relocate the context root, and the caller owns the socket, so only the caller can decide to reconnect.

#### Scenario: Provider relocates the context root
- GIVEN a PROPFIND against the configured context root
- WHEN the server answers 301 with a Location on another authority
- THEN the coroutine yields the redirect and performs no further request
