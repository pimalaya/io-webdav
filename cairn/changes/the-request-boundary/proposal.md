---
cairn: change
id: the-request-boundary
status: landed
created: 2026-11-13
---

# The request boundary

## Why

Second batch from the family audit of 2026-11-13, on what the crate sends and on how it reads the answer to a write. It opens with a correction to the-whole-multistatus.

- Home sets. the-whole-multistatus read the `href` children of a home set property and nothing else, so a server writing the path as bare text, which worked before, lost discovery.
- DAV-04. `read_etag` and the `getetag` reader stripped quotes with `trim_matches('"')`: a weak tag `W/"abc"` came out as `W/"abc`, and went back out as `If-Match: W/"abc`, which is no entity tag (RFC 9110 section 8.8.3). The request side also passed any value starting with `W/` through unquoted, a strong tag of that spelling included.
- DAV-06. A create, update or move returned the caller's string as the id, while the request addressed its percent-encoded form and a listing returns that form: `réunion 1.ics` was created, `r%C3%A9union%201.ics` was listed, and a consumer keying its state by id saw one member vanish and another appear.
- DAV-07. The client trimmed the trailing slash off every calendar and address book path, against RFC 4918 section 5.2, which has clients use the slash form and warns that the other may be redirected. And a redirect met outside discovery surfaced as `WebdavSendError::UnexpectedRedirect`, a unit variant: the target was gone.
- DAV-14. A 207 is a success status, so a DELETE, COPY or MOVE answered 207, which RFC 4918 sections 9.6.1, 9.8.5 and 9.9.4 use to name the members the request failed on, returned `Ok`: `delete_calendar` reported a collection deleted that still existed.
- DAV-11, the half that lives here. Nothing checked a header value before it was written, and a numeric character reference in a multistatus resolves to any character: an entity tag read as `x&#13;&#10;Depth: infinity` and passed back to a conditional write put a second header on the wire. `&#0;` resolved to NUL and `&#x+41;` to a letter.

## What

- `WebdavHrefs` reads the `href` children of a property, and its trimmed text only when it has no `href` child at all, through `deserialize_any`. The concatenation of several hrefs cannot come back: text is read only where there is no href to join.
- One `unquoted` helper behind `read_etag` and `unquoted_etag`: quotes are dropped when the value both starts and ends with one, so a weak tag is kept whole. `entity_tag` passes through `*`, a quoted value and a weak tag with both its quotes, and quotes everything else. A weak tag is sent as it is: under If-Match a conformant server answers 412 (strong comparison, RFC 9110 section 13.1.1), which is the server's answer to give; the precondition is never dropped. A separate entity tag type was not needed, the string form being unambiguous once both ends are checked.
- `sent_id` returns the last segment of the URL a request addresses, and the item and card create and update coroutines and the item move return it. `encode_id` is public, for a name the caller mints: every byte but the RFC 3986 unreserved ones escaped, a form sent as it stands.
- The client's `calendar_path` and `addressbook_path` keep the trailing slash `collection_path` gives them. `WebdavSendError::UnexpectedRedirect` carries `url`, `keep_alive` and `same_origin`.
- `check_members` parses the body of a 207 and fails with `WebdavSendError::MemberFailures`, one `WebdavMemberFailure` per row whose status is not 2xx. `WebdavDelete`, `WebdavCopy`, `WebdavMove` and `CaldavItemMove` go through it. A 207 whose rows are all 2xx stays a success; one that is not a whole multistatus fails as any multistatus does.
- `WebdavSendRaw` and `WebdavFollowRedirects` look at every header value of the request they are given and, on a byte below 0x20 or 0x7F, complete at once with `InvalidHeader`, naming the header, without yielding a byte. The check sits in the two coroutines every request goes through rather than in five builder methods: it covers a request built by hand as well, and no constructor changes its signature. `resolve_reference` leaves a reference to NUL or a signed number unresolved, kept verbatim like any unknown reference.

Closing the header class once, in the HTTP layer's request type, belongs to io-http and is not done here.
