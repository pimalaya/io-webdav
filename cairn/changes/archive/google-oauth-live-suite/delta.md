---
cairn: change
change: google-oauth-live-suite
---

# Delta

## MODIFIED Requirements

### Requirement: Live suites
Ignored integration suites SHALL exercise the full flow against real servers: Radicale and Stalwart from a local script, Fastmail and iCloud from environment credentials, Google from a token minted out of a service account key with domain-wide delegation, so it runs unattended. Providers refusing collection creation (Google, iCloud) SHALL get the same item round trip inside an existing collection, If-Match update and sync-collection from a Depth 0 checkpoint included; Google CardDAV alone skips the sync, its deltas reporting no change made over CardDAV. Each flow SHALL clean up what it created, adopting the id the server answered a create with.
