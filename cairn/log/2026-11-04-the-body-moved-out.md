---
cairn: log
change: the-body-moved-out
landed: 2026-11-04
---

# The body moved out

`WebdavSendRaw` and `WebdavFollowRedirects` hand a successful response's body over once: it moves into `WebdavSendOk::body`, and `WebdavSendOk::response` keeps the status line and headers with an empty body, where both used to hold a copy. PROPFIND, PROPPATCH, REPORT and the calendar and address book home-set lookups read the body through `body_text`, `str::from_utf8` first and `String::from_utf8_lossy` only for bytes that are not UTF-8, the text unchanged.

The copy cost 0.05 to 0.07 cycles per byte and doubled the body in memory while the output lived; the lossy check about 1 cycle per byte, one byte at a time, against 0.06 for `str::from_utf8`. The first speed pass missed the second: its harness measured `str::from_utf8`, which the `pre` row of the scanned multistatus harness now is, the copy gone.

Median cycles per byte over nine interleaved runs on three copies of one binary, CPU 12, for the whole path after the exchange (before: copy, lossy check, typed mapping; after: `body_text`, typed mapping):

| fixture | O3 before | O3 after | `s` before | `s` after |
|---|---|---|---|---|
| multiget1-crref | 2.72 | 1.61 (-41.0%) | 2.91 | 1.80 (-38.1%) |
| multiget500-lf | 2.23 | 1.20 (-46.0%) | 2.29 | 1.27 (-44.5%) |
| multiget500-crlf | 2.74 | 1.74 (-36.4%) | 2.80 | 1.81 (-35.4%) |
| multiget500-crref | 2.50 | 1.49 (-40.3%) | 2.58 | 1.56 (-39.6%) |
| sync5000 | 5.11 | 4.08 (-20.2%) | 5.54 | 4.53 (-18.3%) |
| propfind30 | 7.26 | 6.34 (-12.6%) | 7.87 | 6.97 (-11.4%) |

**Verified.** A unit test of `body_text` (a UTF-8 body borrowed, an invalid byte replaced); the send and follow-redirects tests assert the empty `response.body`, which fails on the copying code. Offline suite, doctests, clippy with `-D warnings`, missing docs, rustdoc links and fmt clean. No caller in the crate or the family read `response.body`.

Capabilities moved: [webdav-core](../spec/webdav-core.md) MODIFIED "Methods".
