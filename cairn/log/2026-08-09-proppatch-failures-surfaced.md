---
cairn: log
change: proppatch-failures-surfaced
landed: 2026-08-09
---

# A PROPPATCH is verified, and an HTTP error no longer dumps a page

Two defects from cardamum's 2026-08-09 iCloud run, both in the RFC 4918 layer.

**A PROPPATCH the server never applied was reported as success.** The coroutine discarded the response body, so nothing looked at what a `DAV:propertyupdate` answers with. Against iCloud, `update_addressbook` on a collection that does not exist returned `Ok(())` and cardamum printed "successfully updated".

The response turned out to be worse than a refusal: iCloud answers 207 with a `200 OK` propstat carrying an **empty** `prop` element, naming nothing, where RFC 4918 §9.2.1 wants a propstat per requested property. Fastmail, checked side by side, lists each property it accepted. So a status-only check would have missed it, and the check that works is: every property the request carried must come back, accepted or refused.

The parser now records the properties of a non-2xx propstat as `WebdavPropFailure { status, property }` on the response entry, next to the 2xx properties it already kept. `WebdavProppatch` returns a `WebdavProppatchOk` carrying the parsed multistatus plus the local names the request asked to change, and the two update coroutines pass it up. `WebdavClientStd::update_addressbook` and `update_calendar` then fail with `PropertiesRejected` for a refused property and `PropertiesIgnored` for one the server never mentioned. The coroutine also traces its response body, as it already traced its request body.

**An HTTP error rendered the raw body.** A Fastmail 404 surfaced its entire HTML error page as the message, while an iCloud 403 with an empty body ended the message at the colon. `WebdavSendError::HttpStatus` and its redirect-aware twin now render a summary: the DAV `responsedescription`, else the HTML `title`, else the body with markup stripped, whitespace collapsed and length capped at 200 characters, with the separator dropped entirely when there is nothing to show. Both variants became struct variants in the process (`{ status, body }`), since thiserror cannot mix a positional field with an expression, and the raw body is still there for consumers that inspect it, as cardamum's `valid-address-data` hint does.

Verified live on both providers: an unknown collection now fails on iCloud with "ignored the property update: displayname, addressbook-description", a real update still succeeds on both, clearing a property still works on Fastmail, and the error messages are one line each.
