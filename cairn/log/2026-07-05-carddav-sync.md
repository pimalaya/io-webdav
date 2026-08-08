---
cairn: log
change: carddav-sync
landed: 2026-07-05
---

# CardDAV read-side sync

Gave the CardDAV layer everything a sync consumer needs: enumerate with a cursor, batch fetch, checkpoint. The push side already existed, so this was read-side only.

The multistatus parser learned to read the top-level sync-token and the response-level status rows, and stopped dropping responses carrying no 2xx propstat, since a removal row is an href plus a 404 and nothing else. Address books gained their ctag and sync token. The sync-collection REPORT landed with its delta type and its dedicated invalid-sync-token error. Batch fetch landed as addressbook-multiget, and ETag-only enumeration as a card reference row with no body.

Capabilities recorded for the first time: webdav-core, carddav, sync.
