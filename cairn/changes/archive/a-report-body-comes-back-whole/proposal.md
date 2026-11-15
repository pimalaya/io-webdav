---
cairn: change
id: a-report-body-comes-back-whole
status: landed
created: 2026-11-15
---

# A report body comes back whole

## Why

Line W3 of the CLI alignment round (.github/cairn/changes/cli-alignment-2026-11-15/tasks.md), found by live testing: on iCloud a GET of a card returns 319 bytes ending in CRLF while an `addressbook-multiget` of the same card returns 317 ending `END:VCARD`, and Fastmail does the same. neverest stores every CardDAV card two bytes short of the server's, so its content hashes never match a GET.

Both servers escape each CR (`END:VCARD&#13;` then a line feed). The scanner resolves the reference, but the typed multistatus trims every text it reads, and `CarddavCardProps::address_data` and `CaldavItemProps::calendar_data` are plain strings read through it: the final CRLF goes with the trim. Every card and item body read by a REPORT passes there: `CarddavCardList` and `CaldavItemList` (the query reports) and both multigets. The sync reports and the enumerations request the ETag alone; a GET returns the body raw.

## What

- A crate-private `rfc4918::typed::whole_text`, a `deserialize_with` reader the typed parser answers with the element's untrimmed text (a newtype name it recognises, as serde_json's `RawValue` does). `address_data` and `calendar_data` read through it.
- The typed parser keeps every text piece of a leaf element; it still skips blank pieces between the children of an element that has some, which trimming drops anyway, so no other text changes.
- A body whose text is blank stays empty data, an unreadable row, as when it was trimmed to nothing.
- The vocabulary-agnostic `parse_multistatus` already kept a property's whole text in `WebdavPropItem::text`; `WebdavResponseEntry::text` trims, as its doc says. Both stay.

## Decisions

- **Whitespace inside the element stays.** RFC 6352 §10.4 and RFC 4791 §9.6 define the response form as `(#PCDATA)` whose value is the address object or the iCalendar object, and XML 1.0 §2.10 has a processor pass every character that is not markup to the application. The only change either RFC allows is the CR an XML parser drops when it normalises CRLF, which they say a client must tolerate. Nothing allows a client to trim, and trimming is what cut the final CRLF that RFC 6350 §3.3 and RFC 5545 §3.4 end every object with. So `<C:address-data>` followed by a line feed and indentation returns that line feed and indentation in the body, the bytes the server sent, the same on every REPORT. iCloud and Fastmail do not indent there (the trim cost them exactly the two bytes of the final CRLF); trimming for a server that did would be a guess at which whitespace is the object's.
- **Line ends stay normalised.** A literal CRLF in the XML still reads as LF (XML 1.0 §2.11), as the RFCs foresee; only `&#13;` yields a CR, which is how both servers keep CRLF.
- **Crate-private, so a patch.** The generic typed rule (text read trimmed) stays for callers' types; no public item changes, and cargo-semver-checks finds none.

## Scope / non-goals

- No version bump: the release (0.9.1) is the owner's.
- cardamum's raw report counts `entry.text(ADDRESS_DATA)` bytes (src/carddav/report/raw.rs:156), trimmed; reading `entry.prop(ADDRESS_DATA)` and its `text` gives the whole size. That is cardamum's line (W2).
