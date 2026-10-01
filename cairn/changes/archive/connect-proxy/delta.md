---
cairn: change
change: connect-proxy
---

# Delta

## ADDED Requirements

## MODIFIED Requirements

### Requirement: Full client
Under one of the TLS features the client SHALL additionally open http and https URLs itself, handling the TCP connection and the TLS negotiation through pimalaya-stream. The connection SHALL go through the proxy the caller names in the connect options, the default resolving it from the environment.

## REMOVED Requirements
