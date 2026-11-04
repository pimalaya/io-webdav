---
cairn: tasks
change: the-multistatus-speed-pass
---

# Tasks

- [ ] Rebuild the harness, rerun `check`, the fuzzer and the cycle table on an idle core; note any drift from the proposal's table
- [ ] `&#13;` fast path in `decode_text`
- [ ] Reserve the raw length before the first decode
- [ ] Block-wise decode (ask the user before `unsafe` or hand-written SIMD)
- [ ] Scanner fast paths for short spans (tag end, whitespace runs, name end)
- [ ] Typed path allocations: `Children::seen`, attribute keys
- [ ] Envelope string matching by length and first byte
- [ ] Ask the user about the success-path body copy in src/rfc4918/send.rs
- [ ] After each lever: `check`, fuzzer, tests, clippy, fmt, cycle table; keep only measured wins
- [ ] Log the pass with its final table, and a handoff for the next one
