---
cairn: tasks
change: proppatch-failures-surfaced
---

# Tasks

- [x] Parser records non-2xx propstat properties as `WebdavPropFailure` on the entry
- [x] `WebdavProppatch` returns the multistatus; the CardDAV and CalDAV update coroutines pass it up
- [x] `WebdavClientStd::update_addressbook` / `update_calendar` fail on a rejected property
- [x] `WebdavSendError::HttpStatus` renders a summarised body, and no trailing colon when empty
- [x] Unit tests for the parser, the client check and the summariser
- [x] cargo fmt, clippy, full test suite
- [x] Fold the delta into the spec, log, archive, CHANGELOG
