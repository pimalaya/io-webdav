---
cairn: log
change: verbatim-card-id
landed: 2026-07-18
---

# Verbatim card ids

Fixed card read, update and delete addressing the wrong resource on servers that name cards with a .vcf suffix (Fastmail, iCloud). A listing stripped the extension while every addressing verb appended it, so an id that did not end in .vcf addressed nothing.

The id became the href's last path segment used verbatim, end to end, which collapsed the reference and entry types onto one field and made the caller own the whole resource name. Creation additionally reads the Location header when the server names the resource itself, as Google does, since that name is what later reads address.

Spec updated: carddav (MODIFIED: card identity), webdav-core (ADDED: resource identity, Location header).
