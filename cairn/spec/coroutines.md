---
cairn: spec
capability: coroutines
status: current
---

# Coroutines

Every network exchange in io-webdav is an I/O-free resumable state machine. The caller owns the socket and feeds the coroutine the bytes it read, so the same logic runs blocking, async, or against in-memory buffers in tests.

### Requirement: Coroutine contract
Every coroutine SHALL implement WebdavCoroutine, pairing a Yield type with a Return type through a two-variant WebdavCoroutineState. Its resume method SHALL take the bytes read since the last step and return either an intermediate yield or a terminal completion.

### Requirement: Standard yield
Standard coroutines SHALL yield WebdavYield, carrying a read request or a write request. They SHALL NOT perform I/O themselves.

### Requirement: Redirect yield
Redirect-capable discovery coroutines SHALL declare their own WebdavRedirectYield, adding a redirect variant that surfaces a 3xx response to the caller with the target URL, the keep-alive flag and the same-origin flag. They SHALL NOT follow the redirect themselves, so the caller decides whether to reconnect to the new authority and retry.

### Requirement: Coroutine chaining
The webdav_try macro SHALL chain an inner coroutine step inside an outer resume, re-yielding intermediate states and short-circuiting terminal errors, the coroutine equivalent of the question mark operator.

#### Scenario: Inner coroutine still wants bytes
- GIVEN an outer coroutine delegating to an inner send coroutine
- WHEN the inner coroutine yields a read request
- THEN the outer coroutine re-yields it unchanged and keeps its own state
