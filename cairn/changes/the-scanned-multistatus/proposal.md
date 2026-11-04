---
cairn: change
id: the-scanned-multistatus
status: landed
created: 2026-11-04
---

# The scanned multistatus

## Why

`parse_multistatus` (src/rfc4918.rs) runs xmlparser, which reads one byte at a time, and every text node then goes through `decode_text`: two `replace` passes and `unescape`, so three allocations and four passes before the text is even appended. Element names are allocated twice (`to_string`, then `clone` into the parent's children), and every element records a child entry and accumulates text, even under structural elements that never surface. The CalDAV mappers then copy each `calendar-data` once more (`as_bytes().to_vec()`).

Consumers also read the result through loops: `entry.text(PROP)`, `has_resource_type`, `children.iter().filter_map(...)`, `supported_reports()`. A serde layer would let a consumer describe the properties it wants as a type and get them parsed.

This change is a proof of concept to measure both before committing to an API: a scanner built for the XML WebDAV servers send, the untyped parser rebuilt on it, and a typed parser deserializing each response's 2xx properties into a consumer's `P`.

## What

The PoC harness held these modules; scan.rs, envelope.rs and de.rs moved into the crate when it landed (see Landed), and the harness now compares the crate with its pre-change version:

- scan.rs: a scanner over `&str` for elements, attributes, text, CDATA, comments and PIs; a DOCTYPE internal subset or malformed markup ends the scan, keeping the prefix. `SIMD` picks memchr kernels or a plain byte loop for the delimiter searches, the rest shared. Text events say whether they hold `&` or CR, found by the same search that finds their end, so clean text is never decoded. Decoding is one pass (CR normalisation and references together).
- envelope.rs: `parse_multistatus` rebuilt on the scanner, returning the crate's `WebdavMultistatus` exactly. Text stays `Cow::Borrowed` until an output field owns it; an element only gathers text and children when something reads them.
- de.rs: `parse_multistatus_as::<P>`, which walks the envelope by hand and deserializes each response's 2xx `<prop>` children into `P`. Children match by local name; text is trimmed and borrowed when clean (`WebdavText`, since serde's `Cow<str>` copies once inside an `Option`); attributes are `@name` keys; consecutive same-named children feed a `Vec`; an empty element is unit, so `Option<()>` is a presence flag; an enum or a map reads children by name.
- consumer.rs: the crate's three mappers (`item_from_entry`, enumerate's `from_entry`, calendar list's `from_entry`) and the same outputs read from typed properties.
- fixture.rs: generated bodies indented like sabre/dav and Radicale output: a 500-event `calendar-multiget` (LF, literal CRLF, and `&#13;` before LF as sabre/dav escapes CR), a 5000-row `sync-collection` with one 404 row in ten, and a 30-calendar PROPFIND with 404 propstats.

`wdxml check` compares every variant with the crate: the untyped rebuild against `parse_multistatus` (Debug output) on the fixtures, seven edge cases (references, broken references, CR, CDATA, comments, mismatched close, truncation, nested `prop`, 403 propstat, nested href) and the depth cap; the typed mappers against the crate's mappers. Everything agrees.

## Results

In-process timing, best of 15 rounds, on an idle core (CPU 12, sibling 28 idle), binary on ext4, release with fat LTO. perf is not installed on the machine today, so these are wall-clock figures and not cycle counts. Time per body in µs, `pre` being the body copy and UTF-8 check the send coroutine already does:

| fixture | bytes | pre | scan scalar | scan SIMD | parse old | parse scalar | parse SIMD | map old | map serde scalar | map serde SIMD |
|---|---|---|---|---|---|---|---|---|---|---|
| multiget1-crref | 1955 | 0.1 | 0.8 | 0.4 | 5.7 | 1.8 | 1.2 | 4.8 | 1.8 | 1.3 |
| multiget500-lf | 762927 | 21.8 | 267.7 | 105.3 | 1619.2 | 562.3 | 246.6 | 1664.4 | 563.8 | 272.6 |
| multiget500-crlf | 785427 | 21.4 | 254.9 | 97.3 | 1851.7 | 695.1 | 425.8 | 1871.4 | 722.2 | 483.8 |
| multiget500-crref | 875427 | 24.5 | 276.7 | 99.7 | 2004.7 | 829.1 | 538.3 | 2065.6 | 863.1 | 592.7 |
| sync5000 | 1172267 | 33.9 | 840.4 | 635.5 | 5994.9 | 1627.6 | 1287.7 | 6395.9 | 1531.2 | 1229.0 |
| propfind30 | 38204 | 0.9 | 20.4 | 19.5 | 193.5 | 79.0 | 77.0 | 205.0 | 66.7 | 65.0 |

What they say:

- The rewrite without SIMD is 2.4 to 3.7 times faster than xmlparser plus the current decode: fewer passes, fewer copies, no unused child entries.
- memchr (SSE2, the only kernel it has without std) adds 2.3 times on LF calendar data, 1.5 to 1.6 times when CR must be decoded, 1.3 times on the markup-heavy sync body and nothing on PROPFIND.
- Together, the end-to-end consumer path (parse plus mapping) is 3.5 to 6.1 times faster on multiget, 5.2 times on sync, 3.2 times on PROPFIND.
- The serde layer costs nothing: the typed path is as fast as the untyped rebuild, faster on sync and PROPFIND, as it builds no `WebdavPropItem`/`WebdavPropChild` tree and borrows hrefs and etags.
- An AVX2 build (`-C target-cpu=x86-64-v3`) moves the rows over 30 KiB by 10% at most, inside this method's noise: not worth a `simd-cpu` dispatch yet.

What they do not say: the absolute cost today is about 2 µs per KiB of body (1.6 to 2 ms for 500 events, 6 ms for 5000 sync rows), while downloading that body takes 100 ms or more. The gain shows on phones (CPU and battery, not latency) and on large first syncs (10000 events, about 40 ms down to 10 ms), not on a desktop round trip.

## Where the time goes now

- Text decode: in the CRLF and `&#13;` rows, parse SIMD minus scan SIMD is 330 to 440 µs, most of it `decode_text` pushing one line at a time (one memchr2 call, one `push_str` and one `push` per line of about 60 bytes). A block-wise decode (copy 16 or 32 bytes, act on the mask of `&` and CR) is the next lever for calendar data.
- Tag handling: on sync, the scanner runs at 1.8 GB/s against 7 to 9 GB/s on text, about 11 tags per row; the name-end search and the quote loop are scalar. A byte-class table or a structural bitmask (simdjson's first stage) would lift it; quotes only matter inside tags, which makes the mask sequential, unlike JSON.
- Allocation: on sync, half the parse is building output; the typed path already borrows, and a borrowed `href` in the untyped entry would need a lifetime on `WebdavMultistatus`.
- Outside parsing: `WebdavSend` copies the body on success (`response.body.clone()`, src/rfc4918/send.rs) and the mappers copy `calendar-data`; both are free to drop.

## Open questions before an API

- Strictness: the scanner validates no names and no character classes, where xmlparser stops early on malformed input. Production needs a differential fuzzer against xmlparser on arbitrary bytes, the prefix rule included.
- A response whose properties fail to deserialize into `P` fails the whole parse today; skipping it with a trace may suit a listing better.
- Same-named children that are not consecutive read as a duplicate field; DAV lists are homogeneous, so this is a documented limit rather than a fix.
- Matching ignores namespaces, as the untyped parser does (the spec's Multistatus parsing requirement), so two properties sharing a local name in different namespaces collide in `P`.
- The coroutines return `WebdavMultistatus`; a typed result needs `P` on `WebdavPropfind` and `WebdavReport`, or a coroutine returning the body for the caller to parse with `parse_multistatus_as`.
- memchr in a `no_std` build only has its SSE2 kernel on x86_64 (and NEON on aarch64); a crate-owned kernel on `simd-cpu` would pick AVX2 at run time, which the measurements above do not justify yet.

## Landed

Wired into the crate on 2026-11-04:

- src/rfc4918/xml.rs is the scanner, memchr only. Two fixes came out of the differential fuzzer: a DOCTYPE is skipped with its internal subset, as xmlparser did (the PoC stopped there, returning an empty multistatus, which a listing reads as a mass removal), and a repeated `name` attribute keeps its last value, as before.
- `parse_multistatus` runs on it with unchanged output; xmlparser is gone.
- src/rfc4918/typed.rs holds `parse_multistatus_as`, `WebdavMultistatusOf` and `WebdavResponseOf`. Settled while wiring: repeated property runs keep the first (as `entry.prop()` did, and two 2xx propstats naming one property no longer fail on a duplicate field); a tuple field reads its element's children and an enum field its first child, since the field's own name never names a variant; ignored elements are traced as `trace_unrecognized` did; decoded text is trimmed in place (copying it cost 25% on `&#13;` bodies). `WebdavText` was dropped: coroutine results are owned, so props are `String`.
- `WebdavPropfind<M>` and `WebdavReport<M>` parse into `M`, `WebdavMultistatus` by default, through the sealed `WebdavMultistatusParse` (the user's pick over body-returning coroutines). A response whose properties do not fit `P` fails the parse with `WebdavMultistatusParseError::InvalidProps`: skipping it would read as a removal.
- The CalDAV and CardDAV item list, multiget and enumerate, the calendar and address book lists and sync-collection read typed props. PROPPATCH, the principal and the home-set lookups stay untyped, their props being caller-chosen or nested hrefs.

The harness (`old.sh` extracts 39bb548 as io-webdav-old) checks the crate against its old self: fixtures and edge cases agree, and `wdxml fuzz` ran 2.4M mutated bodies (seeds 1 to 6), 522k of which xmlparser tokenizes, with no mismatch and no panic. Timings, same method as above (µs per body):

| fixture | parse old | parse new | map old | map new |
|---|---|---|---|---|
| multiget1-crref | 5.6 | 1.4 | 5.8 | 1.2 |
| multiget500-lf | 1638.7 | 264.2 | 1668.2 | 273.3 |
| multiget500-crlf | 1740.8 | 438.8 | 1833.4 | 501.4 |
| multiget500-crref | 1919.7 | 546.7 | 1977.5 | 615.7 |
| sync5000 | 6593.4 | 1302.0 | 6977.0 | 1378.3 |
| propfind30 | 210.8 | 76.7 | 225.2 | 74.6 |

Left for later: the speed levers are handed off in change the-multistatus-speed-pass; a check on a bare-metal target is still owed (none installed here; memchr builds without std).
