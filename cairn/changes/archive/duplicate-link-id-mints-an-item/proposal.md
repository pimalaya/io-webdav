---
cairn: change
id: duplicate-link-id-mints-an-item
status: landed
created: 2026-08-28
---

# A refused duplicate UID says so

> Cross-repo change, same id in eight repositories. This crate's part is small and independent: it can land before or after the storage work, and nothing here waits on io-replica.
>
> Order: **pimdir** → **io-replica** → **io-pimdir** → **io-webdav** (here) → **neverest** → **himalaya**, **cardamum**, **calendula**.

## Why

A collection may hold two resources under one `UID`, though RFC 4791 §4.1 and RFC 6352 §5.1 both forbid it and §6.3.2 forbids the `PUT` that would create one. Verified 2026-08-28 on a Posteo calendar: four `UID`s under two hrefs each.

A consumer replicating such a collection onto another server pushes both copies, deliberately: it has no basis for choosing which one the user wanted, and the server it is pushing to is the authority on whether it will hold two. A conforming one refuses the second with `409` carrying the `CALDAV:no-uid-conflict` or `CARDDAV:no-uid-conflict` precondition, which is a complete, actionable answer: this resource was refused, for this reason, and the user can fix the source.

That answer currently arrives as `WebdavSendError::HttpStatus { status: 409, body }`, indistinguishable from a quota refusal, a lock, or any other conflict. The consumer can only report a number, and a number is not something a user can act on. The crate already knows how to name a precondition rather than a status: `unsupported_report` reads the RFC 3253 §3.6 `DAV:supported-report` element out of the body, whatever status wraps it, and surfaces a dedicated error. This is the same shape for the write path.

## What

- **A dedicated error for the refusal.** A `PUT` (create or update) answered with a status carrying `no-uid-conflict` SHALL surface as its own error variant, distinct from every other send failure, carrying the status and the body as the others do.
- **Matched on the precondition, not the status.** RFC 4791 §5.3.2 and RFC 6352 §6.3.2 both name the element; the status they recommend is `409`, and servers have been seen wrapping preconditions in others. The element is the fact, exactly as `unsupported_report` argues for its own.
- **Both flavours, one variant.** The two elements differ only in their namespace, and no consumer of this crate has a reason to tell a calendar refusal from a card one: the flavour is already known from which client method was called.

## Scope / non-goals

- **No retry, no rename, no fallback.** The crate reports the refusal and does nothing about it. Choosing to re-`UID` a resource, to skip it, or to stop is the consumer's, and mutating a body to get past a server's validation is not something this layer may do.
- **No other precondition.** `no-uid-conflict` is named because a consumer acts on it. The rest stay as summarised statuses until one earns the same argument.
- **No change to the create path's `If-None-Match`.** A duplicate `UID` is a different refusal from a duplicate resource name, and both keep their own signal.
