---
cairn: change
id: live-suite-standard-refusals
status: landed
created: 2026-10-01
---

# Live suites assert the standard refusals

## Why

Measured with tarpaulin on 2026-10-01, the live suites (Google, Stalwart, Radicale) reached 76.8% of the library. The gaps were the paths where providers differ and consumers get hurt: `WebdavClientStd::connect` itself, the 412 on a stale If-Match, the invalid sync token, the duplicate UID refusal, collection PROPPATCH and query filters. All were covered offline only, against canned responses.

## What

Test code only:

- every flow opens its client through `WebdavClientStd::connect`;
- after the If-Match update, a second update with the stale ETag must be refused with 412;
- a sync from a token no server issued must be refused as invalid, or answered with a full resync that holds the live resource; an empty delta fails;
- the full flows (the run owns the collection) PUT a second resource under the same UID and expect no-uid-conflict, and rename the collection with PROPPATCH;
- the card flows query the card by the UID the server holds;
- `carddav_cards` takes `CarddavCardsChecks { sync, if_match }`, Google CardDAV opting out of both.
