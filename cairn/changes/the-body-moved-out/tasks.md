---
cairn: tasks
change: the-body-moved-out
---

# Tasks

- [x] Move the body out in `WebdavSendRaw` and `WebdavFollowRedirects`, the `response` doc saying so
- [x] `body_text` in the five multistatus coroutines, with its unit test
- [x] Assert the empty `response.body` in the send and follow-redirects tests
- [x] Measure the path after the exchange at O3 and `opt-level = "s"`
- [x] Fold the delta into spec/webdav-core.md, log, CHANGELOG, the second-pass handoff
