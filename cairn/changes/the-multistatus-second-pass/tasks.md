---
cairn: tasks
change: the-multistatus-second-pass
---

# Tasks

- [ ] Snapshot the baseline (base.sh), rebuild both sides at O3 and `opt-level = "s"`, A/A check, rerun the first pass's table
- [ ] Build the pushed response in place, or show it cannot be
- [ ] Measure the allocation share on a scudo-like allocator before choosing allocation levers
- [ ] Ask the user about `unsafe` spare-capacity writes in the decoder, with numbers
- [ ] After each lever: `wdxml check`, both fuzzers, the lanes tests on four targets, Miri, clippy, fmt, the table at both levels
- [ ] Log the pass with its tables, and a handoff for the next one
