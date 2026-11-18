---
cairn: log
change: the-server-side
landed: 2026-11-18
---

# The server side

The crate gains a server side for Pimalaya Server, which serves CalDAV and CardDAV over its own HTTP stack and consumes the crate by path. The feature set is split to let it take the server without the client.

**Features.** The old `client` feature, which gated the blocking client and std, is renamed `std`. `client` now gates the coroutines (`coroutine`, `rfc5397`, `rfc6578`, the method submodules of `rfc4918`, `rfc4791` and `rfc6352`, `WebdavAuth`, `emit_header`, `read_etag`) and makes io-http optional; it is on by default and implied by `std` and the TLS features. `server` gates the new `server` module. The vocabulary stays compiled under no feature: namespaces, property constants, hrefs and ids, the multistatus parsers and the request body builders. The client-only private helpers carry their own `cfg`. The live suites gate on the TLS features, which `WebdavClientStd::connect` needs.

**Server module.** `server::body` reads a request body into a namespace-resolved element tree over the existing scanner (which gains an `offset` accessor under `server`), and cuts raw elements with every inherited namespace declared on them. `propfind`, `proppatch`, `mkcol` (`MKCALENDAR` too), `lock` and `report` parse their bodies; `header` parses the WebDAV headers; `multistatus` and `precondition` write the 207 and `DAV:error` bodies; `name::WebdavPropName` types the properties. The addressbook-query filter reads into `rfc6352::filter::CarddavFilter`, so the client and server sides share one model.

**Found while testing.** XML normalises CRLF to LF, so a calendar-data written as plain escaped text lost its carriage returns: the writer encodes CR as `&#13;`. The fuzzer found a namespace declaration named by a malformed attribute (a prefix holding `"` and `>`) re-emitted into a raw element, and an element named with a control character that the scanner cannot close again: both now fail as malformed.

**Capabilities moved.** server: added. packaging: "no_std core", "Feature layering" and "Source layout" modified. client: feature names modified.

**Verified.** tests/server.rs (21 tests) parses the bodies the client builds (PROPFIND, PROPPATCH, MKCOL, MKCALENDAR, both multigets, a calendar-query filter, a full addressbook-query filter, sync-collection) back to their meaning, runs `CaldavCalendarList`, `CaldavItemList` and `WebdavSyncCollection` against written multistatus bodies, reads written error bodies back as `InvalidSyncToken` and `DuplicateUid`, and parses the request examples of RFC 4918, 4791, 6352 and 6578; unit tests cover the tree, the names and every header. The fuzz target fuzz/fuzz_targets/server_bodies.rs ran 60 s clean (about one million runs) after the two fixes above. fmt, clippy over all features and default (all targets) and over no features, `client`, `server` and `std` (lib), tests with all, default and server-only features, and rustdoc with broken links denied are clean.

**Not done.** calendar-data and address-data requests keep their partial-retrieval children as raw XML (`expand` aside); no bare-metal target build was run; the LOCK body supports the write lock type only, the one RFC 4918 defines.
