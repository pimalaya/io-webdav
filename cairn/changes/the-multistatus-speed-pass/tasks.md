---
cairn: tasks
change: the-multistatus-speed-pass
---

# Tasks

- [x] Rebuild the harness, rerun `check`, the fuzzer and the cycle table on an idle core; note any drift from the proposal's table
- [x] `&#13;` fast path in `decode_text`
- [x] Reserve the raw length before the first decode
- [x] Block-wise decode (tried, slower than run copies; SIMD probes landed instead)
- [x] Scanner fast paths for short spans (tag end, whitespace runs, name end)
- [x] Typed path allocations: `Children::seen` (attribute keys left, rare)
- [x] Envelope string matching by length and first byte (byte-string matches, `same_name`)
- [ ] Ask the user about the success-path body copy in src/rfc4918/send.rs (carried to the-multistatus-second-pass)
- [x] After each lever: `check`, fuzzer, tests, clippy, fmt, cycle table; keep only measured wins
- [x] Log the pass with its final table, and a handoff for the next one
