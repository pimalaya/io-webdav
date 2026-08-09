---
cairn: change
id: proppatch-failures-surfaced
status: landed
created: 2026-08-09
---

# A refused PROPPATCH reports success, and an error dumps a whole HTML page

Two defects found by cardamum's 2026-08-09 iCloud run, both in the RFC 4918 layer.

**A PROPPATCH the server refuses is reported as success.** `WebdavProppatch` discards the response body, so nothing ever looks at the per-property status a `DAV:propertyupdate` answers with (RFC 4918 §9.2). iCloud answers 207 with the failure inside the multistatus when the collection does not exist, so `update_addressbook` on an unknown collection returns `Ok(())` and cardamum prints "successfully updated". Fastmail happens to 404 at the HTTP level, which is why the gap stayed hidden.

The parser cannot help as it stands: it keeps properties from 2xx propstats only and drops the rest, so a wholly refused response arrives as an entry with empty props and no trace of why.

**A non-2xx status renders the raw body.** `WebdavSendError::HttpStatus` prints the body verbatim, so a Fastmail 404 surfaces its entire HTML error page as the message, while an iCloud 403 with an empty body ends the message at the colon. Every consumer inherits both.

## What changes

- The parser records the properties a non-2xx propstat carried, as `WebdavPropFailure { status, property }` on the response entry, alongside the 2xx properties it already keeps.
- `WebdavProppatch` returns the parsed multistatus instead of `()`, and the two update coroutines (CardDAV addressbook, CalDAV calendar) pass it up.
- `WebdavClientStd::update_addressbook` and `update_calendar` fail with a new `PropertiesRejected` error when the response carries any failure.
- `WebdavSendError::HttpStatus` renders a summary rather than the raw body: the DAV `responsedescription` when there is one, else the body with its markup stripped and its whitespace collapsed, capped; an empty body drops the trailing colon entirely. The variant keeps the raw body, so a consumer inspecting it (cardamum's `valid-address-data` hint) is unaffected.
