---
cairn: log
change: the-whole-multistatus
landed: 2026-11-13
---

# The whole multistatus

Five defects from the family audit of 2026-11-13 (DAV-01, DAV-03, DAV-05, DAV-08, DAV-15), all on the path between a server's answer and what a sync consumer takes for the state of a collection.

**A multistatus is whole or it is an error (DAV-01).** `parse_multistatus` and `parse_multistatus_as` fail unless the body is one whole `multistatus` document: the first element is `multistatus`, it closes, every end tag matches, no element follows it and the scanner reached the end of the body (`XmlScanner::is_done`, the scanner stopping short on malformed markup). `WebdavMultistatusParseError::UnexpectedRoot` carries the root found, `Incomplete` the count of responses read. Before, an HTML page answered with a 200 parsed as zero rows, a document cut mid-way as the responses closed before the cut, an empty body as zero rows, and every listing coroutine handed that on as a complete answer. A closed `multistatus` holding no response is still an empty listing. Text outside the root is ignored, so a byte order mark or a notice printed around the document changes nothing; an element before the root fails. The status is still checked for success only, a 200 carrying a whole multistatus being accepted.

**The envelope by position (DAV-03).** `Tag::of` takes the parent's tag: `response` counts under the root, `href`, `status` and `propstat` under a response, `prop` and `status` under a propstat. A property value nesting those names, as an expand-property answer does, used to replace the response being read in `parse_multistatus`, while the typed parser read it right. The audit's larger option, one envelope shared by both parsers, was left: the typed envelope drops the indentation before a property's content and keeps an element's text apart from its children's, which would change `WebdavPropItem::text` for every caller.

**Home sets (DAV-05).** `CaldavCalendarHomeSet` and `CarddavAddressbookHomeSet` parse a typed multistatus and take the first non-empty `href` child (`WebdavHrefs`), where the flattened text of a property holding two hrefs resolved to one URL naming neither. A home set written as bare text, with no `href` element, is no longer read. Returning every home set is left open: it changes the return type and the client's discovery cache.

**Preconditions (DAV-08).** `valid-sync-token` and `supported-report` go through `has_element` like the other preconditions. The first is matched under any status, a 409 included; neither is raised by a body that only quotes the name, `supported-report-set` in a refusal's description for one.

**Self entry (DAV-15).** The enumeration compares the row's path with the request path through `same_path`, percent-decoded on both sides without allocating (`decoded` is now an iterator, `collection_id` using the same comparison), and src/rfc6578/sync_collection.rs uses the `href_path` of src/rfc4918.rs, which also drops a query or fragment, in place of its own.

**Verified.** Each regression test failed on the unfixed code first: both parsers accepted the five bodies (the page, the cut, the unclosed root, the element after the root, the empty body), the nested response surfaced with the inner href, the home sets came back as `/dav/calendars/alice/%20…/dav/calendars/shared/`, the 409 surfaced as `HttpStatus`, the quoted names raised `UnsupportedReport` and `InvalidSyncToken`, the encoded self entry was listed, and the fallback enumeration of a cut body listed 1 of 2 members. Tests that pinned the prefix of a malformed body now assert the error: `parse_multistatus_fails_at_a_mismatched_close`, `parse_multistatus_fails_on_malformed_xml`, and the typed prefix test, replaced by `parse_multistatus_as_fails_on_a_body_that_is_not_a_whole_multistatus`; the typed depth test nests under a `multistatus` root, and the typed reading-rules fixture lost its mismatched last response. Offline suite, clippy with `-D warnings`, missing docs, rustdoc links, fmt and the `--no-default-features` build clean. No timing was taken: the hot loops gained a flag and a comparison per element, and `Tag::of` compares names under three parents only.

Capabilities moved: [webdav-core](../spec/webdav-core.md) MODIFIED "Multistatus parsing" and "Typed multistatus"; [sync](../spec/sync.md) MODIFIED "Invalid token" and "Unsupported report", ADDED "Self entry"; [discovery](../spec/discovery.md) MODIFIED "Home set resolution".
