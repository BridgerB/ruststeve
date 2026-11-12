---
title: Determinism rules
description: Every source of nondeterminism the core must not touch, the fix for each, and the clippy.toml that enforces it.
type: reference
tags: [determinism, replay, lint, clippy]
aliases: [clippy.toml, banned types, disallowed methods, no HashMap]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@a02c9a0
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#{ path = \"std::collections::HashMap\", reason = \"random iteration order breaks replay: use BTreeMap or IndexMap\" },"
  - "ruststeve:FUNCTIONAL.md#{ path = \"std::time::Instant::now\",    reason = \"core is deterministic: use Input.tick\" },"
  - "ruststeve:FUNCTIONAL.md#A run is reproducible when the same config, seed and recorded inputs always produce byte-identical outputs."
  - "ruststeve:FUNCTIONAL.md#One more source, found while testing this guide's code:"
  - "ruststeve:FUNCTIONAL.md#CommandId, TaskId and ExpectationId come from counters inside Brain, never from UUIDs or the clock, so a replay produces the same IDs and diffs line up."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[ordered-collections]]"
  - "[[recordings]]"
---

# Determinism rules

A run is reproducible when the same config, seed and recorded inputs always produce byte-identical outputs. One stray `HashMap` iteration or clock read breaks that, so the rules are enforced by lint and by CI replay, not by care.

## Key files
- `FUNCTIONAL.md`, section 4 (the `clippy.toml`), section 5.5 and 5.6, section 9.1 (the checklist).

## How it works

| Source | Fix | Enforced by |
|---|---|---|
| Wall clock | read `Input.tick` only | `disallowed-methods`: `Instant::now`, `SystemTime::now`, `thread::sleep` |
| Randomness | one `ChaCha8Rng` in `Brain`, seeded from the recording header, serde-checkpointed | `disallowed-methods`: `rand::thread_rng`, `rand::rng` |
| Map/set order | `BTreeMap`, `BTreeSet`, `IndexMap` | `disallowed-types`: `HashMap`, `HashSet` |
| Shared state | none | `disallowed-types`: `RefCell`, `Rc` |
| Environment | config lives in the recording header | `disallowed-methods`: `std::env::var` |
| Sort ties | total keys: `total_cmp`, then `EntityId` | review |
| Floats across platforms | replay on the same target; `libm` for `sin`/`atan2` if needed | header records the target triple |
| Version drift | commit `Cargo.lock`; header has git commit and MC version | replay refuses a mismatched header without `--force` |
| Parallelism | no `rayon` in the core | `cargo tree` check |
| Internal ids | `CommandId`, `TaskId`, `ExpectationId` from counters in `Brain` | review |

A `clippy.toml` lives in each core crate (`data`, `protocol`, `world`, `physics`, `memory`, `plan`, `sim`, [[workspace-crates]]) because clippy reads the one found from the crate's directory upward; it also sets `allow-unwrap-in-tests = true`.

## Gotchas
- Any text form of a recording (the `replay export` view) must round-trip floats exactly: the guide found `serde_json` drifts in the last bit unless `float_roundtrip` is on. The recording itself is binary ([[recording-format]]).
- Reject NaN at the boundary: a NaN `Vec3` from a bad packet is a parse error, never a value the core sees.

## See also
- [[ordered-collections]] - the decision behind the map ban.
- [[recordings]] - the header that carries seed, config and versions.
- [[cc-randomness-on-the-tick]] - how clojurecraft keeps its randomness replayable.
