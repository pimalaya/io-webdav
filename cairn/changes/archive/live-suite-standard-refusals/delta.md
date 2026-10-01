---
cairn: change
change: live-suite-standard-refusals
---

# Delta

## MODIFIED Requirements

### Requirement: Live suites
Ignored integration suites SHALL exercise the full flow against real servers: Radicale and Stalwart from a local script, Fastmail and iCloud from environment credentials, Google from a token minted out of a service account key with domain-wide delegation, so it runs unattended. Every flow SHALL open its client through `WebdavClientStd::connect`. Providers refusing collection creation (Google, iCloud) SHALL get the same item round trip inside an existing collection, sync-collection from a Depth 0 checkpoint included. Beyond the happy path, the flows SHALL assert the standard refusals: a stale If-Match update refused with 412, an unknown sync token refused as invalid or answered with a full resync (never an empty delta), and, where the run owns the collection, a duplicate UID refused with no-uid-conflict. The full flows SHALL also rename their collection with PROPPATCH, and the card flows SHALL query a card by the UID the server holds. Google CardDAV alone skips the sync and the stale If-Match checks, its deltas reporting no change made over CardDAV and its PUT ignoring If-Match. Each flow SHALL clean up what it created, adopting the id the server answered a create with.
