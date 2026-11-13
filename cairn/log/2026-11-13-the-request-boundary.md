---
cairn: log
change: the-request-boundary
landed: 2026-11-13
---

# The request boundary

A correction to the-whole-multistatus, then five more defects from the family audit of 2026-11-13: DAV-04, DAV-06, DAV-07, DAV-14 and the local half of DAV-11.

**Home sets, corrected.** the-whole-multistatus made the home set lookups read `href` children only, and a home set written as bare text stopped being found. `WebdavHrefs` is now a hand-written `Deserialize`: through `deserialize_any` an element holding text alone reads as that text, trimmed, and an element holding children reads its `href` run, every other child ignored. Text is read only where no `href` exists, so two hrefs never join again.

**Entity tags (DAV-04).** `read_etag` and `unquoted_etag` share `unquoted`, which drops the quotes only when the value starts and ends with one: `W/"abc"` stays whole where `trim_matches('"')` made it `W/"abc`. `entity_tag` passes `*`, a quoted value and a weak tag carrying both quotes, and quotes the rest, a bare value starting with `W/` included. A weak tag goes out unchanged under If-Match, where a conformant server answers 412; the precondition is never dropped. One visible effect: a stored tag of the mangled form `W/"abc` no longer equals the one read, so such a member reads as modified once.

**Ids as sent (DAV-06).** `sent_id` gives the last segment of the URL a request addresses; `CaldavItemCreate`, `CaldavItemUpdate`, `CaldavItemMove`, `CarddavCardCreate` and `CarddavCardUpdate` return it, a `Location` header still winning on create. `rfc4918::encode_id` escapes every byte but the unreserved ones, for a name the caller mints, and the result is sent as it stands. An id of plain unreserved characters is unchanged.

**Collections with their slash, redirects with their target (DAV-07).** The client's `calendar_path` and `addressbook_path` no longer trim the slash `collection_path` returns, so every listing, enumeration, multiget and sync addresses the collection as RFC 4918 section 5.2 asks. The item and card paths were joined after a trim of their own and did not move. `WebdavSendError::UnexpectedRedirect` carries `url`, `keep_alive` and `same_origin`; the client still wraps it as a send error outside discovery, and raises its own `UnexpectedRedirect(Url)` in discovery.

**A 207 on DELETE, COPY, MOVE (DAV-14).** `check_members` parses the body and returns `WebdavSendError::MemberFailures` with one `WebdavMemberFailure` (href, status) per row whose status is not 2xx, a row without a status counting as failed. `WebdavDelete`, `WebdavCopy`, `WebdavMove` and `CaldavItemMove` call it, which covers the calendar, address book, item and card deletes built on `WebdavDelete`. A 207 whose rows are all 2xx is a success; a 207 that is not a whole multistatus fails with the parse error.

**Header values and character references (DAV-11).** `WebdavSendRaw` and `WebdavFollowRedirects` check every header value of the request they are built with; a byte below 0x20 or 0x7F puts them in a `Reject` state that completes with `InvalidHeader`, naming the header, and yields nothing. Checking there rather than in the five `WebdavRequest` setters covers a request built by hand and changes no constructor. `resolve_reference` takes digits only and refuses NUL, so `&#0;`, `&#x+41;` and `&#+65;` stay verbatim. CR and LF still resolve, XML allowing them; the header check is what stops them at the wire. The same check belongs once in the HTTP layer's request type, which is io-http's to add.

**Verified.** Every regression test failed on the unfixed code first: the bare-text home sets resolved to nothing, the weak tag read `W/"etag-2`, the bare `W/abc` went out unquoted, the creates, updates and move returned `réunion 1.ics` and `zoé 1.vcf`, the client sent `REPORT /dav/calendars/personal HTTP/1.1`, the redirect error printed no target, the three 207 answers returned `Ok`, the request carrying `User-Agent: ua\r\nX: y` was yielded for writing, and `&#0;` decoded to NUL. `CaldavItemMove` and `WebdavFollowRedirects` were wired last, their tests seen failing while the rest already passed. Tests matching the unit `UnexpectedRedirect` now match the struct variant, and `send_raw_rejects_redirects` asserts its three fields. Offline suite, clippy with `-D warnings`, missing docs, rustdoc links, fmt and the `--no-default-features` build clean. The live suites were not run: the trailing slash now goes out on every collection request and has yet to be seen against the five providers.

Capabilities moved: [webdav-core](../spec/webdav-core.md) MODIFIED "Multistatus parsing", "Resource identity" and "Location header", ADDED "Entity tags", "Member failures", "Header values" and "Unexpected redirect"; [client](../spec/client.md) MODIFIED "No redirect following", ADDED "Collection paths"; [discovery](../spec/discovery.md) MODIFIED "Home set resolution".
