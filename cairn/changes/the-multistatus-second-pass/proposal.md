---
cairn: change
id: the-multistatus-second-pass
status: active
created: 2026-11-04
---

# The multistatus second pass

## Why

This is the handoff for the next session on io-webdav's multistatus speed, after the first pass (change the-multistatus-speed-pass, log 2026-11-04-the-multistatus-speed-pass). It says where the parse stands, where its time goes now, the levers left and what they need, how to measure, and what must not move. Read the log first: it says what landed and what was tried and dropped.

## State of the repository

Nothing is committed since 39bb548 (0.6.0). The working tree holds three things in sequence, all uncommitted: the scanned multistatus (change the-scanned-multistatus), the first speed pass, and the `:name` attribute fix found during it. Commit, release and version bump are the user's.

## Where it stands

Cycles per byte on this machine (harness cycles.sh, O3, best of three) for 0.6.0 and now; the typed column is the end-to-end mapper path the crate's CalDAV and CardDAV coroutines take:

| fixture | parse 0.6.0 | parse now | typed 0.6.0 | typed now |
|---|---|---|---|---|
| multiget1-crref | 10.09 | 1.34 | 10.48 | 1.61 |
| multiget500-lf | 9.50 | 1.06 | 10.30 | 1.19 |
| multiget500-crlf | 10.46 | 1.64 | 11.20 | 1.73 |
| multiget500-crref | 10.43 | 1.39 | 10.94 | 1.47 |
| sync5000 | 25.29 | 4.09 | 27.00 | 4.15 |
| propfind30 | 24.42 | 7.22 | 26.66 | 6.27 |

The body copy and UTF-8 check every coroutine does before parsing cost 0.13 c/B, about 10% of a typed LF multiget now.

## Where the time goes

Cycle shares of the typed path at `opt-level = "s"` (perf record, grouped by function):

| fixture | envelope and scanner | decode | allocator | typed and serde | memcpy | memcmp | mapper |
|---|---|---|---|---|---|---|---|
| multiget500-lf | 34% | 5% | 21% | 8% | 5% | 2% | 7% |
| multiget500-crlf | 25% | 32% | 15% | 6% | 7% | 1% | 5% |
| multiget500-crref | 25% | 28% | 16% | 8% | 8% | 2% | 4% |
| sync5000 | 46% | 0% | 17% | 13% | 2% | 5% | 11% |
| propfind30 | 23% | 1% | 20% | 29% | 3% | 4% | 11% |

The mapper column is the harness's copy of the crate's mappers (`id()`, unquoted etags, `collection_id`); the typed column on PROPFIND is mostly the caller's serde derive and its `BTreeMap`s. The glibc allocator here is cheaper per call than Android's scudo, so the allocator share understates the phone.

## Levers, ranked

1. **The success-path body copy** (src/rfc4918/send.rs, `response.body.clone()`). Half of the 0.13 c/B before parsing; moving the body out leaves `WebdavSendOk::response.body` empty, a visible change to the send coroutine's output. The user decides.
2. **The response push** in `parse_multistatus_as`: the `WebdavResponseOf` is built on the stack, its 4-byte `status` store then read inside a 16-byte load when it moves into the list, a store-forwarding stall worth about 6% of sync at O3. Reserving first and pushing one response late did not help; a fix needs the struct built where it lands.
3. **Allocations.** Per response: the href, every string property, the list's growth; the mappers then copy the id and the unquoted etag. Trimming the etag in place measured under 1% (the `drain` costs what the allocation did). Worth measuring on an allocator like scudo before judging.
4. **Field matching in serde derive.** Under fat LTO, `match value { "getetag" => .. }` in derived field visitors compiles to `bcmp` calls: about 5% of sync and 4% of PROPFIND. The crate could hand identifiers to `visit_borrowed_bytes`, which derived visitors match inline, but hand-written identifier visitors without `visit_bytes` would then fail: not done.
5. **Decoder copies.** Each run between rewritten bytes is one `push_str`, a memcpy call: 7 to 8% of CRLF and `&#13;` rows at `opt-level = "s"`. Writing 16-byte blocks into the string's spare capacity would remove them, at the cost of an `unsafe` beyond the SIMD intrinsics: a question for the user first.
6. **The untyped parser on markup** (17% faster on sync at `opt-level = "s"` against 27% for the typed path): it keeps `Vec<OpenElement>` pushes and a `pop_if` closure; it reads small bodies only (PROPPATCH, principal, home sets), so low priority.

Measured not worth it: an AVX2 or AVX-512 path through simd-cpu (a `-C target-cpu=native` build slowed CRLF by 8% and sync by 7%), block decoders writing a byte buffer, and portable mask folds left to the vectorizer (not vectorized inside loops, and unrolled poorly at `opt-level = "s"`).

## How to measure

The harness of change the-scanned-multistatus compares with 0.6.0 (`wdxml check`, `wdxml fuzz`, cycles.sh, profile.sh); this change's harness/ compares two versions of the crate:

```sh
cd cairn/changes/the-multistatus-second-pass/harness
./base.sh [rev]                      # baseline: a git revision, or the working tree
./build.sh base0 base small          # wdxml against the baseline, opt-level "s"
./build.sh tree0 tree small          # against the working tree
./cpuload.sh                         # pick an idle core, CPU 12 by default
./multi.sh 15 "base0 tree0" map-new/sync5000 parse-new/multiget500-crlf
./dfuzz.sh 1000000 1                 # differential fuzzer, six seeds off the measuring core
cd lanes && nix-shell ../cross.nix --run "cargo test --target armv7-unknown-linux-musleabihf"
nix-shell ../miri.nix --run "cargo miri test --target aarch64-unknown-linux-gnu"
```

- build.sh builds both sides through one symlinked path (target/wdxml-ab/cur), so two builds of one source are byte-identical, and copies three binaries to target/wdxml-bins on ext4; multi.sh rotates the copies and prints medians. Profiles: release (O3) and small (`opt-level = "s"`, what the Android app ships). Measure both: the first pass's first designs won at O3 and lost at `s`.
- diff/ (`wddiff`) compares the tree with the baseline on every mutated body, malformed ones included, untyped and typed into seven shapes; failing bodies land in diff/target/. Built for aarch64, armv7 or i686 with cross.nix, it runs under qemu (`qemu-aarch64 -cpu neoverse-n1`, `qemu-arm -cpu cortex-a15`, `qemu-i386`).
- lanes/ mounts src/rfc4918/xml.rs and the lanes backends through symlinks, so their tests run on other targets (cross.nix) and under Miri (miri.nix), which the crate's own tests cannot: its dev-dependencies link for the host only.
- perf sampling works here: `nix-shell -p perf --run "perf record -F 20000 -e cycles:u ..."` then `perf report --no-children --sort symbol`, and `perf annotate -s <symbol>` for the hot instructions.

## Guardrails

- Outputs stay identical: `wdxml check`, `wdxml fuzz` at zero mismatch and panic over seeds 1 to 6 and 11 to 16, and `dfuzz.sh` at zero mismatch against the baseline, on x86_64 and with the backend changed on the targets it runs (aarch64 NEON, armv7 SWAR, i686 SSE2).
- Every lanes backend reports the first lane exactly; the lanes test checks every byte at every lane on each.
- `cargo test --all-features`, clippy with `-D warnings` on all targets and on the library for aarch64, armv7 and i686, missing docs, rustdoc links and fmt stay clean.
- The API stays (generic `WebdavPropfind<M>` and `WebdavReport<M>`, one `new`); no new dependency; `unsafe` only for the intrinsics, each block with its SAFETY line; anything else is the user's call.
- Keep only measured wins, at both O3 and `opt-level = "s"`; a lever under 5% needs an A/A check and fifteen runs.

## Machine and traps

- Zen 5, CPUs 0 to 7 and 8 to 15 the two CCDs, sibling of n at n+16; measure on an idle core (CPU 12 was) and keep builds and fuzzers off it.
- The app ships arm64-v8a, armeabi-v7a, x86_64 and x86. 32-bit ARM takes the SWAR backend, its NEON intrinsics being unstable in Rust; 32-bit x86 shares the SSE2 one.
- `opt-level = "s"` leaves closures, `Vec::extend_from_slice`, `str` slicing and loops over a few needles out of line or rolled: write compares out, inline the small helpers, test there.
- Under fat LTO, string `match` arms call `bcmp`; byte-string patterns stay inline.
- A struct built with narrow stores and moved right away stalls store-to-load forwarding; perf annotate shows it as a hot store right after a 16-byte load.

## Open items outside this pass

- neverest needs the type annotation at src/dav/client.rs:335 when it re-pins io-webdav (from the scanned multistatus).
- A bare-metal build (`thumbv7em-none-eabihf`) is unchecked; the SWAR backend is what it would use.
- Live suites not run against these changes.
