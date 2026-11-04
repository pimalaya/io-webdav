---
cairn: delta
change: the-body-moved-out
---

# Delta

## ADDED Requirements

## MODIFIED Requirements

### Requirement: Methods
The crate SHALL provide one coroutine per WebDAV method: PROPFIND, PROPPATCH, MKCOL, COPY, MOVE, DELETE, GET, PUT, OPTIONS and REPORT, plus the low-level send coroutine they all build on. The send coroutine SHALL hand a successful response's body over once, apart from the response head, so no body is held twice. PROPFIND and REPORT SHALL parse their multistatus into the representation the caller picks, vocabulary-agnostic (Multistatus parsing) or typed (Typed multistatus).

## REMOVED Requirements
