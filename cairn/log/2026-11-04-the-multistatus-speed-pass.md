---
cairn: log
change: the-multistatus-speed-pass
landed: 2026-11-04
---

# The multistatus speed pass

A speed pass over the scanned multistatus of the same day, measured at both optimisation levels a build uses: O3, and `opt-level = "s"`, which the Android app builds with and which inlines and unrolls far less. Outputs are unchanged.

**Scanner.** Names, end tags and the first 64 bytes of text are probed 16 bytes at a time through the new src/rfc4918/xml/lanes.rs: SSE2 on x86 and x86_64, NEON on aarch64, both baseline where the build enables them, and two 64-bit words (SWAR) elsewhere, 32-bit ARM included, since its NEON intrinsics are not stable. The probe that ends a name also finds its first colon, so the local name costs nothing more; longer text still goes to memchr. Each compare is written at its call site and the scanner is inlined, as a closure or a loop over needles stays a call or a loop at `opt-level = "s"`. A first portable version, a fold LLVM was meant to vectorize, was not vectorized inside the decoder loop and ran 20 to 135% slower; the backends use intrinsics, each behind an `unsafe` block whose safety is the build's target feature.

**Decoder.** Text events carry the length of their clean prefix, so decoding starts at the first `&` or CR; the output is reserved once; `&#13;` and the five predefined references skip the general path; runs between rewritten bytes are probed like text and pushed whole. A 64-byte block decoder copying into a byte buffer was slower at both levels: one call per block at `opt-level = "s"`, and a UTF-8 pass to rebuild the `String`.

**Envelope.** `parse_multistatus_as` walks the body through a non-generic `Envelope`, one response at a time, so the inlined scanner exists once rather than once per property type; the generic part only deserializes and pushes. Node slots are reused across responses and reset in place, text is appended through `Cell::get_mut`: both avoided copying structs right after building them, which stalled store-to-load forwarding. A map's seen names stay inline up to eight, then spill into a `BTreeSet`, linear where every new name used to scan all the earlier ones.

**Comparisons.** Under fat LTO, `match s { "response" => .. }` compiles to a `bcmp` call per arm and `match s.as_bytes() { b"response" => .. }` to inline loads: both parsers match envelope names as bytes, the untyped one through a `Tag` computed once per element. End tags and child groups compare names through an inline `same_name`. `status_code` reads `HTTP/1.1 ddd` by bytes, other lines as before.

**Fix found on the way.** A wider differential run (seeds 11 to 16) against 0.6.0 found an attribute with an empty prefix, `:name='VEVENT'`, read as `name` by 0.6.0 (xmlparser gives it an empty prefix) and as no name by the scanned multistatus. `attr` now treats an empty prefix as none; a regression test fails without the fix. The scanned multistatus being unreleased, no released behaviour changed.

**Not kept.** An AVX2 or AVX-512 path through simd-cpu: a `-C target-cpu=native` build left LF multiget unchanged and slowed CRLF by 8% and sync by 7%, the work being short spans and per-element logic, so no runtime dispatch was added. Moving the built response into the list one response late (no gain), reserving before the push (no change), and trimming entity tags in place (under 1%, the `drain` costing what the allocation did).

**Results.** Median cycles per body over 15 interleaved runs on three copies of each binary (ext4, CPU 12), against the tree before the pass; an A/A column of two identical binaries stayed within 2%.

| fixture | typed O3 | untyped O3 | typed `s` | untyped `s` |
|---|---|---|---|---|
| multiget1-crref | -43.5% | -47.6% | -40.9% | -44.0% |
| multiget500-lf | -32.4% | -36.6% | -30.5% | -30.3% |
| multiget500-crlf | -36.4% | -37.3% | -35.9% | -34.3% |
| multiget500-crref | -51.0% | -52.4% | -52.4% | -50.1% |
| sync5000 | -23.7% | -24.2% | -27.4% | -17.0% |
| propfind30 | -31.9% | -21.9% | -33.4% | -22.3% |

Against 0.6.0 (harness cycles.sh, best of three, cycles per byte, O3): parse 10.09 to 1.34 (multiget1-crref), 9.50 to 1.06 (LF), 10.46 to 1.64 (CRLF), 10.43 to 1.39 (`&#13;`), 25.29 to 4.09 (sync), 24.42 to 7.22 (PROPFIND); the typed mappers 10.48 to 1.61, 10.30 to 1.19, 11.20 to 1.73, 10.94 to 1.47, 27.00 to 4.15, 26.66 to 6.27. The SWAR backend, forced on x86_64 where the old code had memchr's SSE2 kernels, is 15 to 27% faster than the old code at `opt-level = "s"` and 10 to 24% at O3. The harness binary at `opt-level = "s"` shrank by 4 KB.

**Verified.** `wdxml check`; the old-versus-new fuzzer on seeds 1 to 6 and 11 to 16 (4.8M bodies, 1.05M tokenized by xmlparser) without mismatch or panic; a differential fuzzer against a frozen copy of the tree before the pass, malformed bodies included and typed parses into seven shapes compared, 6M bodies on SSE2, 3.6M with SWAR forced on x86_64, 480k on aarch64 NEON, 400k on armv7 SWAR and 450k on i686 SSE2 under qemu, no mismatch; it caught a planted one-character bug at about 2,200 bodies in 20,000. The xml and lanes unit tests on x86_64, i686, aarch64 and armv7 (qemu, release and debug) and under Miri for the four. The offline suite, doctests, clippy with `-D warnings` (all targets and features, the bare build, and the library for aarch64, armv7 and i686), missing docs and rustdoc links clean. Live suites not run.

Capabilities moved: none; outputs are unchanged.
