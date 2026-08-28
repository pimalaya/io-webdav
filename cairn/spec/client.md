---
cairn: spec
capability: client
status: current
---

# Standard blocking client

The optional std layer, gated behind the client feature: a ready-made pump for callers who want a working client rather than a coroutine to drive.

### Requirement: Light client
The client SHALL wrap a single stream the caller opened, any blocking reader and writer, and expose one method per WebDAV operation. It SHALL also expose the stream, so a higher-level crate can pump its own coroutines against the same connection while reusing the discovery cache.

### Requirement: Full client
Under one of the TLS features the client SHALL additionally open http and https URLs itself, handling the TCP connection and the TLS negotiation through pimalaya-stream.

### Requirement: TLS providers
The crate SHALL offer Rustls with ring crypto as the default, Rustls with aws crypto, and native-tls, each as its own feature implying the client feature.

### Requirement: Discovery cache
The client SHALL cache the principal URL and both home sets, plus the reports each listed collection advertises. Each discovery step SHALL resolve the previous one when it is not cached. A method needing a home set that was never resolved SHALL fail with a dedicated missing-cache error rather than guess a path.

### Requirement: No redirect following
The client SHALL never follow a redirect. It owns a single connected stream, so it SHALL surface the target URL in an unexpected-redirect error and let the caller reconnect and swap the stream in.

#### Scenario: Context root moved
- GIVEN a client configured with a context root the provider redirects away from
- WHEN discovery runs
- THEN the call fails with the redirect target, and the caller reconnects there and retries
