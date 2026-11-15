---
cairn: log
change: a-report-body-comes-back-whole
landed: 2026-11-15
---

# A report body comes back whole

A card or item body read by a REPORT keeps its final CRLF: the whole text of `address-data` or `calendar-data` is returned, untrimmed, so it matches what a GET of the same resource returns (line W3 of the CLI alignment round).

**The defect.** iCloud and Fastmail escape each CR of a body (`END:VCARD&#13;` then a line feed). The typed multistatus trims every text it reads, and the card and item props read `address-data` and `calendar-data` through it, so `CarddavCardList`, `CaldavItemList` and both multigets cut the final CRLF: on iCloud a GET returned 319 bytes, a multiget 317, and neverest stored every CardDAV card two bytes short.

**The fix.** A crate-private `rfc4918::typed::whole_text`, a `deserialize_with` reader the typed parser answers with the element's untrimmed text, reads both data properties. The parser keeps every text piece of a leaf element, still skipping blank pieces between the children of an element that has some, so every other text reads as before. Blank data stays an unreadable row. The vocabulary-agnostic parser already kept the whole text in `WebdavPropItem::text`, `WebdavResponseEntry::text` trimming it; both stay.

**Whitespace inside the element stays.** RFC 6352 §10.4 and RFC 4791 §9.6 make the response form `(#PCDATA)` whose value is the object itself, and XML passes every character that is not markup through; the only change the RFCs allow is the CR dropped by XML line-end normalisation, which stays (a literal CRLF reads as LF, `&#13;` as CR). An indented element therefore returns its indentation.

**Capabilities moved.** carddav: "A card comes back whole" added. caldav: "An item comes back whole" added. webdav-core: "A parsed property keeps its whole text" added.

**Verified.** `a_card_keeps_its_address_data_whole` (tests/rfc6352.rs) and `an_item_keeps_its_calendar_data_whole` (tests/rfc4791.rs) run a listing and a multiget over a body ending in an escaped CRLF, an indented one and a blank one; both fail with the whole read neutralised (the trimmed text read instead) and pass with it. cargo-semver-checks 0.47.0 (nixpkgs rustc 1.95.0) against the v0.9.0 tree, all features, as a patch release: 222 checks pass, no semver update required. fmt, tests with all and with no default features, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, and `cargo deny check` are clean. No live suite ran.
