---
cairn: change
id: no-std-xml-parser
status: landed
created: 2026-09-28
---

## MODIFIED Requirements

### Requirement: no_std core
The crate SHALL be no_std unconditionally, pulling in alloc for its owned buffers. std SHALL be reachable only through the client feature, and no dependency SHALL pull it in otherwise: the core SHALL build for a bare-metal target.
