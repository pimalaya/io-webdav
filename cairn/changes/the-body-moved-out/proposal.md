---
cairn: change
id: the-body-moved-out
status: landed
created: 2026-11-04
---

# The body moved out

## Why

After the first speed pass, the step between the HTTP exchange and the parse cost more than the parse of a calendar multiget. `WebdavSendRaw` and `WebdavFollowRedirects` copied the body (`response.body.clone()`), so every successful exchange held it twice, in `WebdavSendOk::response` and in `WebdavSendOk::body`, until the caller dropped the output: 0.05 to 0.07 cycles per byte, and twice the body in memory, 20 MB at peak for a 10 MB first sync. Next to it, PROPFIND, PROPPATCH, REPORT and the two home-set lookups read the body with `String::from_utf8_lossy`, which checks one byte at a time: about 1 cycle per byte where `str::from_utf8` takes 0.06. The first pass's harness measured `str::from_utf8`, so neither cost showed in its tables.

No caller inside the crate reads `response.body` (they take the status and the ETag, Location or DAV headers), nor does any crate of the family; the field is public, though, and its doc already called it the head.

## What

- `WebdavSendRaw` and `WebdavFollowRedirects` move the body into `WebdavSendOk::body`; `WebdavSendOk::response` keeps the status line and headers, its `body` empty. Breaking for a caller reading `response.body`, in a release breaking already.
- The multistatus coroutines read the body through `body_text`: `str::from_utf8`, borrowed, and `String::from_utf8_lossy` only when the bytes are not UTF-8, so the text is the same.
- The scanned multistatus harness's `pre` row measures what the coroutines now do before parsing, the UTF-8 check alone.
