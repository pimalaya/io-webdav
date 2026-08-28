---
cairn: log
change: duplicate-link-id-mints-an-item
landed: 2026-08-28
---

# A refused duplicate `UID` says so

A collection may hold two resources under one `UID`, which RFC 4791 §4.1 and RFC 6352 §5.1 both forbid, and which a Posteo calendar was found doing on 2026-08-28: four `UID`s under two hrefs each. A consumer replicating such a collection pushes both copies deliberately, having no basis for choosing between them, and the server it pushes to is the authority on whether it will hold two. This is that server's answer, named.

**The refusal has a name.** `WebdavSendError::DuplicateUid` carries the status and the raw body, like the two send failures beside it. The item and card create and update coroutines classify their own failure through `duplicate_uid`, the shape `WebdavReport` already uses for `UnsupportedReport`: the precondition in the body, whatever status carries it. RFC 4791 §5.3.2 and RFC 6352 §6.3.2 both name the element and only recommend the 409, and a 409 carrying none of it is any of the other conflicts a write meets, so the element is what is matched. It is read as an element, out of `quick_xml`, rather than searched for as a substring, so a body merely quoting the words is not a refusal. The prefix is ignored, as everywhere else in the crate, which is how both flavours of the element reach the one variant: no consumer needs to tell a calendar refusal from a card one, the flavour being known from the method it called.

**The classifier lives under the PUT.** `duplicate_uid` sits in `rfc4918::put`, the layer where both flavours meet, next to the coroutine whose failures it reads. It is the one place in the RFC 4918 module naming a CalDAV and CardDAV element, which the error variant it returns already does.

**The client names it too.** `WebdavClientStdError::is_duplicate_uid` sits beside `is_unsupported_report`, matching the one wrapper a PUT failure can arrive in: no write coroutine follows redirects, so unlike a REPORT there is no second path to catch. A consumer already matching `err.is_unsupported_report()` reads this one the same way, rather than reaching through the wrapper for a nested variant.

**The crate does nothing about it.** No retry, no rename, no fallback: choosing to re-`UID` a resource, to skip it or to stop is the consumer's call, and mutating a body to get past a server's validation is not something this layer may do. The create path's `If-None-Match: *` is untouched, a duplicate `UID` and a duplicate resource name being two refusals with two signals.

Capabilities moved: webdav-core (the refusal is named), caldav and carddav (their preconditions carry it). Offline coverage grew by three tests: one per flavour, each pinning the canned 409 the RFCs describe, a 409 whose description merely spells the words, and a 507 wrapping the same precondition, plus one running the client predicate over a create and an update.
