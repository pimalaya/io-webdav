---
cairn: change
id: the-multistatus-speed-pass
status: landed
created: 2026-11-04
---

# The multistatus speed pass

## Why

This is the handoff for a session taking the lead on io-webdav's multistatus speed. It is self-contained: what was done on 2026-11-04 and why, the state of the repository, where the hot code lives, what is measured, where the time goes, the levers ranked, how to measure, and what must not move. The pass itself changes no behaviour: output stays identical to the crate before the scanner.

Read with it: change the-scanned-multistatus (proposal, with its "Results" and "Landed" sections), log 2026-11-04-the-scanned-multistatus, and spec webdav-core ("Multistatus parsing", "Typed multistatus").

## State of the repository

Nothing is committed (HEAD is 39bb548, "build: prepare v0.6.0"; 0.6.0 is the last release). The working tree holds the whole scanned-multistatus change: new src/rfc4918/xml.rs and src/rfc4918/typed.rs, modified src/rfc4918.rs, propfind.rs, report.rs, src/lib.rs, the CalDAV, CardDAV and sync-collection mappers, two test files, Cargo.toml (xmlparser out, memchr in), CHANGELOG, spec, log, and the two change folders. Git writes need the user's permission.

## What was done

1. **Survey.** io-webdav is the only family crate parsing XML on a hot path (io-pim-discovery's autoconfig parses one small document per setup with serde-xml-rs). The old parser ran xmlparser (byte at a time), decoded every text node with two `replace` passes plus `unescape` (three allocations, four passes), allocated each element name twice, recorded child entries and text under structural elements nobody reads, and the CalDAV mappers copied each `calendar-data` once more.
2. **Proof of concept** (the-scanned-multistatus/harness, first version): a scanner with a scalar and a memchr build, the untyped parser rebuilt on it, and a typed serde parser, measured against the crate. Findings that still guide this pass: the scalar rewrite alone was 2.4 to 3.7 times faster (fewer passes and copies); memchr (SSE2, the only kernel it has without std) added 2.3 times on LF text, 1.5 to 1.6 times when CR must be decoded, 1.3 times on markup-heavy sync bodies and nothing on PROPFIND; an AVX2 build (`-C target-cpu=x86-64-v3`) moved rows by 10% at most, within noise; the serde layer cost nothing (it builds no property tree).
3. **Wiring.** The user chose generic coroutines over body-returning coroutines and over separate constructors: `WebdavPropfind<M = WebdavMultistatus>` and `WebdavReport<M>` parse into `M` through the sealed `WebdavMultistatusParse`, with a single generic `new`. This is source-breaking where a call site never names the result type (rustc does not apply a type default during inference): the tests got `let r: WebdavReport = ...`, and downstream only neverest src/dav/client.rs:335 needs it (carillon and cardamum name the type). Claude's calls while wiring: a response whose properties do not fit `P` fails the whole parse with `WebdavMultistatusParseError::InvalidProps` (a dropped row reads as a removal, the mass-delete class); repeated property runs keep the first, as `entry.prop()` did; a tuple field reads its element's children, an enum field its first child; ignored elements are traced as `trace_unrecognized` did; decoded text is trimmed in place; props are owned `String`s (coroutine results cannot borrow the body). The crate's mappers (CalDAV and CardDAV list, multiget, enumerate; calendar and address book lists; sync-collection) read typed props; PROPPATCH, the principal and the home-set lookups stay untyped.
4. **Verification.** The harness was rebuilt around the real crates: `old.sh` extracts 39bb548 as io-webdav-old, `check` compares outputs on fixtures and edge cases, and a mutation fuzzer compares old and new on every mutated body xmlparser tokenizes fully and runs the typed parsers on all of them for panics. It found two differences, both fixed: a DOCTYPE internal subset ended the new scan (an empty multistatus, so a mass removal; now skipped as before, entities unexpanded), and a repeated `name` attribute kept the first value (now the last, as before). Final run: 2.4M bodies, 522k tokenized by xmlparser, no mismatch, no panic. Offline suite, doctests, clippy and fmt are clean; typed.rs and xml.rs have unit tests; the only uncovered lines in them are serde plumbing nothing reaches. Live suites were not run (the user reports the Radicale suite failing on CI for an unrelated reason).
5. **Measurement for this handoff.** perf counting works (`nix-shell -p perf`) but sampling is refused here, so the cycle table below comes from perf stat and the breakdown from callgrind. Both are scripted in the harness (cycles.sh, profile.sh).

## Where the hot code lives

- src/rfc4918/xml.rs: `XmlScanner::next` (text runs by `memchr3` on `<`, `&`, CR; start tags by a scalar name-end search then a `memchr3` loop over `>` and quotes; end tags by `memchr`; comments, CDATA and PIs by `memmem`; DOCTYPE skipped byte by byte), `XmlAttrs`, `attr`, `decode_text` (one pass, `memchr2` on `&` and CR per span), `decode_attr`, `resolve_reference`.
- src/rfc4918/typed.rs: `parse_multistatus_as` (envelope frames, a per-response node arena linked by first child and next sibling, 2xx propstats spliced onto the response root), `push_text`, `Node::take_text`, and the deserializer (`NodeDe`, `Group`, `Siblings`, `Children` with its `seen` list and `@name` keys).
- src/rfc4918.rs: `parse_multistatus` (the untyped parser, an open-element stack that only gathers text and children something reads), `push_text`, and the typed props shared by the mappers (`WebdavEtagProps`, `WebdavChildNames`, `WebdavReportSet`).
- src/rfc4918/send.rs and report.rs, propfind.rs: the body is cloned on success (`response.body.clone()`), then `String::from_utf8_lossy` runs before `M::parse`.

## Where it stands

Cycles per body (perf stat `cycles:u`, difference between two iteration counts, best of three, CPU 12 with sibling 28 idle), and cycles per byte; `./cycles.sh` reprints it:

| fixture | bytes | parse old | parse new | map old | map new | pre |
|---|---|---|---|---|---|---|
| multiget1-crref | 1955 | 20085 (10.27 c/B) | 5090 (2.60) | 20994 (10.74) | 5479 (2.80) | 243 (0.12) |
| multiget500-lf | 762927 | 7503131 (9.83) | 1284571 (1.68) | 7980914 (10.46) | 1266019 (1.66) | 96482 (0.13) |
| multiget500-crlf | 785427 | 8385467 (10.68) | 2074944 (2.64) | 9073553 (11.55) | 2104311 (2.68) | 101812 (0.13) |
| multiget500-crref | 875427 | 9300973 (10.62) | 2568355 (2.93) | 9878517 (11.28) | 2599890 (2.97) | 116328 (0.13) |
| sync5000 | 1172267 | 30651848 (26.15) | 6459510 (5.51) | 32813693 (27.99) | 6443986 (5.50) | 160097 (0.14) |
| propfind30 | 38204 | 976897 (25.57) | 343228 (8.98) | 1051310 (27.52) | 349156 (9.14) | 4140 (0.11) |

- parse: `parse_multistatus` into `WebdavMultistatus`, old (39bb548, xmlparser) and new (scanner).
- map: what the crate's mappers return (items, refs, calendars), old over untyped entries, new through `parse_multistatus_as` and typed props, as src/rfc4791 and src/rfc6352 do now.
- pre: the body copy and UTF-8 check the coroutines do before parsing.
- Fixtures (harness src/fixture.rs), indented like sabre/dav and Radicale output: 500 events of about 1.5 KiB each with VTIMEZONE and VALARM, line ends as LF, literal CRLF, or `&#13;` before LF (sabre/dav escapes CR); 5000 sync-collection rows, one in ten a 404; 30 calendars with every list property and 404 propstats on a third.

Two shapes: text-heavy bodies (multiget, 1.7 to 3 c/B) and markup-heavy ones (sync, PROPFIND, 5.5 to 9 c/B). The typed path costs no more than the untyped one. In absolute terms the old parser cost about 2 µs per KiB, under network time for any body; the gain matters for phones (CPU, battery) and large first syncs, which is the bar for how far this pass should go.

## Where the time goes

callgrind instruction counts over the measured calls only, grouped by function (`./profile.sh map-new <fixture>`). Instructions, not cycles: a 16-byte SIMD step counts as one, so scanning weighs less here than in cycles.

| fixture | scan | decode | `&#13;` refs | typed envelope + de | allocator | inlined std (slicing, iterators) | UTF-8 check | memcmp | memcpy |
|---|---|---|---|---|---|---|---|---|---|
| multiget500-lf | 36% | 9% | 0% | 16% | 15% | 13% | 5% | 2% | 2% |
| multiget500-crlf | 23% | 27% | 0% | 11% | 18% | 11% | 3% | 1% | 4% |
| multiget500-crref | 21% | 26% | 13% | 8% | 13% | 8% | 3% | 5% | 3% |
| sync5000 | 41% | 0% | 0% | 20% | 14% | 17% | 1% | 2% | 2% |
| propfind30 | 33% | 1% | 1% | 24% | 19% | 14% | 2% | 3% | 1% |

## Levers, ranked

1. **`&#13;` fast path** (xml.rs `decode_text`). Every line of a sabre/dav calendar body carries `&#13;`, and `resolve_reference` reaches the numeric branch only after string compares against `amp`, `lt`, `gt`, `quot`, `apos`, then parses decimal: 13% of instructions on crref, plus most of its memcmp share. Test `&#13;` (and `&#10;`, `&#xD;`) by bytes before the general path. Expected: crref down 10 to 15%.
2. **Reserve before decoding** (typed.rs `push_text`, rfc4918.rs text handling). Decoded text grows line by line through `realloc` (realloc and `_int_realloc` about 4% on crref). Reserve the raw length on the first dirty piece. Expected: a few percent on CRLF bodies.
3. **Block-wise decode.** `decode_text` calls `memchr2` once per line (about 60 bytes) then `push_str` and `push`: per-call setup dominates on short spans. Copying 16 or 32 bytes at a time and acting on the mask of `&` and CR (SSE2 is baseline on x86_64, NEON on aarch64) would remove the per-line calls. Largest single gain left on CRLF and `&#13;` bodies (decode is 27% there); needs hand-written SIMD (see Guardrails) or a scalar formulation LLVM vectorises.
4. **Short-span fast paths in the scanner** (`XmlScanner::next`). In markup-heavy bodies every tag runs a scalar `position` for the name end, then `memchr3` for `>` or a quote even when the next byte is `>`, and every whitespace run between tags runs `memchr3` over a few bytes. Check `rest[name_end] == b'>'` and `/>` before searching; probe the first bytes of a text run inline before calling memchr; find the name end with a byte-class table. Scan is 33 to 41% of sync and PROPFIND.
5. **Allocations in the typed path** (typed.rs). Per map, `Children::seen` allocates a `Vec` on its first key (once per response and per nested struct); every attribute key allocates a `String` for `@name` (`OwnedKey`). `seen` can be a fixed array with a spill; the attribute key can be built in a stack buffer and passed with `visit_str`, which field identifiers accept. Allocator is 14 to 19% on sync and PROPFIND, part of it the owned `href` and props a coroutine result needs anyway.
6. **Envelope string matching.** `frames.pop_if` compares every end tag's local name with memcmp, and Start matches locals against `"response"`, `"href"`, `"status"` and so on; dispatch on length and first byte first. memcmp is 2 to 5%.
7. **The body copy before parsing** (send.rs). `pre` is 0.13 c/B, now 8% of an LF multiget; dropping the clone halves it. It changes `WebdavSendOk`'s shape: ask the user.

Not worth it on these numbers: a simdjson-style structural index (quotes only matter inside tags, so the mask is sequential, and the markup-heavy cost sits in per-element work rather than in finding delimiters); AVX2 dispatch (within noise, see What was done).

## How to measure

The harness is cairn/changes/the-scanned-multistatus/harness/, a standalone crate (its own `[workspace]`, outside the package) linking the working tree as io-webdav and 39bb548 as io-webdav-old:

```sh
cd cairn/changes/the-scanned-multistatus/harness
./old.sh                                   # extracts 39bb548 into target/io-webdav-old
nix-shell ../../../../shell.nix --run "cargo build --release && cargo build --profile fuzz && cargo build --profile prof"
./target/release/wdxml check               # outputs agree with 39bb548
./target/fuzz/wdxml fuzz 400000 <seed>     # no mismatch, no panic; one seed per core
./cycles.sh 12 map-new parse-new           # cycle table on CPU 12, given variants
./profile.sh map-new sync5000              # callgrind breakdown, grouped by group.pl
taskset -c 12 ./target/release/wdxml time 15     # wall clock, best of 15 rounds
./target/release/wdxml run <variant> <fixture> <n>   # one row
```

- Variants: `pre`, `parse-old`, `parse-new`, `map-old`, `map-new`. Fixtures: `multiget1-crref`, `multiget500-lf`, `multiget500-crlf`, `multiget500-crref`, `sync5000`, `propfind30`.
- Profiles: `fuzz` is release with debug assertions and overflow checks; `prof` is release with debug info, for callgrind. `run` is `#[inline(never)]` so `--toggle-collect='wdxml::run'` keeps fixture generation out of profiles.
- Fuzz seeds 1 to 6 at 400000 each were the acceptance run; failing bodies land in target/fuzz/ (`wdxml show <file>` prints both parses).
- The PoC's scalar variant and its own scanner, envelope and serde modules were dropped from the harness once the crate held them; their numbers are in the-scanned-multistatus proposal.

## Guardrails

- Output stays identical: `wdxml check` clean and the fuzzer at zero mismatch and zero panic over six seeds of 400000 after every change.
- `cargo test --all-features`, clippy on all targets and `cargo fmt` stay clean (through `nix develop --command` in the crate); typed.rs and xml.rs keep their unit tests passing.
- The API stays: generic `WebdavPropfind<M>` and `WebdavReport<M>` with one `new` (the user's choice).
- no_std with alloc; memchr is the only parsing dependency. A new dependency, `unsafe`, or hand-written SIMD is a question for the user first. If SIMD is written, the CPU token comes from the application through simd-cpu (`SimdCpu::current()`), never detected inside the library.
- Keep only measured wins; a lever below 5% needs the rigour below before it counts. Log the pass with its final table and leave a handoff for the next one, as the codec passes did.

## Machine and traps

- AMD Ryzen AI MAX+ 395 (Zen 5), 16 cores, 32 threads: CPUs 0 to 7 and 8 to 15 are the two CCDs, the SMT sibling of CPU n is n+16. CPU 12 (sibling 28) was idle on 2026-11-04.
- Other sessions run long suites here: an io-pimdir test run took 11 cores mid-measurement and doubled every row. Check `ps -eo pid,pcpu,psr,etime,args --sort=-pcpu` and /proc/loadavg before each table, and keep builds and fuzzers off the measuring core (the fuzz slices used CPUs 9 to 14).
- Binary placement moves rows by 5 to 15%: run A/B comparisons from ext4 (the harness target/ is), keep several copies of each binary with an A/A check under 1%, and build with `-C llvm-args=-align-all-functions=6 -C llvm-args=-align-loops=64` when comparing two builds of the crate (the harness release profile does not set them yet).
- perf is `nix-shell -p perf` (older notes point at a store path that was collected); `perf stat` works, `perf record` is refused. callgrind handles the SSE2 code memchr runs without std.

## Open items outside this pass

- neverest needs the type annotation at src/dav/client.rs:335 when it re-pins io-webdav.
- A bare-metal build (`thumbv7em-none-eabihf`) is unchecked: no such target is installed here.
- Live suites not run against the change; the Radicale CI failure predates it.
- Commit, release and version bump are the user's.

## Landed

The pass ran on 2026-11-04 and is logged in 2026-11-04-the-multistatus-speed-pass, with its tables and checks. The ranked levers, as they turned out:

1. `&#13;` fast path: landed with the other predefined references, in the decoder rather than in `resolve_reference`.
2. Reserve before decoding: landed.
3. Block-wise decode: tried as 64-byte then 16-byte block copies into a byte buffer, slower than probing for the next `&` or CR and pushing whole runs, which landed.
4. Scanner fast paths: landed as 16-byte probes (src/rfc4918/xml/lanes.rs), plus `/>` and `>` read straight after a name and the colon found by the same probe.
5. Typed path allocations: the seen list stays inline up to eight names; attribute keys still allocate (rare in DAV bodies, not measured).
6. Envelope string matching: landed as byte-string matches and `same_name`, the `bcmp` calls coming from fat LTO.
7. The success-path body copy: not touched, it changes `WebdavSendOk`; carried to the second pass for the user.

Beyond the list: the scanner inlined into a non-generic `Envelope`, node slots reused across responses, and the measurements taken at `opt-level = "s"` as well, which reversed two early designs. perf sampling works here now (`perf record -e cycles:u`), contrary to the trap above. The next levers are handed off in change the-multistatus-second-pass.
