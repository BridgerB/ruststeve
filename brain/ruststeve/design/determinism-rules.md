---
title: Determinism rules
description: Every source of nondeterminism the core must not touch, the fix for each, the clippy.toml that enforces it, and the lane seed the planner may read only to break ties.
type: reference
tags: [determinism, replay, lint, clippy]
aliases: [clippy.toml, banned types, disallowed methods, no HashMap, lane seed, tie-break, Opts.seed]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@ac53fd87
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#{ path = \"std::collections::HashMap\", reason = \"random iteration order breaks replay: use BTreeMap or IndexMap\" },"
  - "ruststeve:FUNCTIONAL.md#{ path = \"std::time::Instant::now\",    reason = \"core is deterministic: use Input.tick\" },"
  - "ruststeve:FUNCTIONAL.md#A run is reproducible when the same config, seed and recorded inputs always produce byte-identical outputs."
  - "ruststeve:FUNCTIONAL.md#One more source, found while testing this guide's code:"
  - "ruststeve:FUNCTIONAL.md#CommandId, TaskId and ExpectationId come from counters inside Brain, never from UUIDs or the clock, so a replay produces the same IDs and diffs line up."
  - "ruststeve:crates/plan/src/lane_seed.rs#pub enum Use"
  - "ruststeve:crates/plan/src/lane_seed.rs#pub fn derive"
  - "ruststeve:crates/plan/src/lane_seed.rs#pub fn pick"
  - "ruststeve:crates/plan/src/lane_seed.rs#pub fn from_tick"
  - "ruststeve:crates/plan/src/lane_seed.rs#pub const DEFAULTS: Self = Self { seed: Some(0) };"
  - "ruststeve:crates/plan/src/lib.rs#fn lane_seed_watch"
  - "ruststeve:crates/bot/src/lib.rs#pub seed: Option<u64>,"
  - "ruststeve:crates/bot/src/bin/race.rs#seed: Some(plan::lane_seed::derive(&a.run, a.lane, i)),"
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
| Planner ties | the lane seed, through `plan::lane_seed::pick` only (below) | `Use` is a closed enum; review |
| Internal ids | `CommandId`, `TaskId`, `ExpectationId` from counters in `Brain` | review |

A `clippy.toml` lives in each core crate (`data`, `protocol`, `world`, `physics`, `memory`, `plan`, `sim`, [[workspace-crates]]) because clippy reads the one found from the crate's directory upward; it also sets `allow-unwrap-in-tests = true`.

## The lane seed
The shell's tick seeds are a xorshift started from `Opts.seed`; with none, from the clock (`now_ms() | 1`), as before. A race gives every bot its own: `lane_seed::derive(run, lane, bot)`, an FNV-1a of the run id mixed with the lane and the bot's index through splitmix64, never 0. The same race rerun breaks its ties alike, and the lanes of one snapshot break them apart (they used to play one game to tick 18,700: lava-portal wall 3 on `issue-83-lava-portal`).

The planner holds the first non-zero tick seed for the run (`Plan.lane`, `Draw::from_tick`), so a tie broken once stays broken and no choice flips from tick to tick. A world with seed 0 (every hand-built test world) keeps the old defaults.

**The tie-break rule.** The planner reads the seed only to choose among options it holds equal: never a threshold, a count, a distance or a safety rule. Every use is a variant of `lane_seed::Use`, drawn through `lane_seed::pick(seed, use, key, n)`, and writes a ledger note when it breaks the tie. Grow `Use` by accretion; renumbering a salt changes a recorded run's draws. The uses today:

| Use | The tie | Note |
|---|---|---|
| `StartQuarter` | the explore heading's first quarter, 0, 90, 180 or 270 (it was always 0, +z); only while the heading is still 0 | "tie broken by the lane seed: explore heading starts at H, turns of T", once a run |
| `TurnSide` | a failure's quarter turn, +90 or +270; either reaches all four in four turns (`Plan::turned`, every heading turn) | the same note |

The heading also steers the mine fallback's level tunnel (the nearest cardinal) and the stairs, so lanes from one snapshot dig apart from their first tunnel. Tests: `crates/plan/tests/lane_seed.rs`.

Two sim fixtures were fitted to heading 0 and failed on the seed's heading (probe 46161667574): the plains log 30 out (`crates/bot/tests/planner.rs`) and the tunnel walls in the rock (`crates/bot/tests/tunnel.rs`). They hold `Draw::DEFAULTS` (the old defaults, whatever the ticks carry), because they test the code behind the tie, not the tie. A new fixture should pass on every start quarter or say why not.

## Gotchas
- Any text form of a recording (the `replay export` view) must round-trip floats exactly: the guide found `serde_json` drifts in the last bit unless `float_roundtrip` is on. The recording itself is binary ([[recording-format]]).
- Reject NaN at the boundary: a NaN `Vec3` from a bad packet is a parse error, never a value the core sees.

## See also
- [[ordered-collections]] - the decision behind the map ban.
- [[recordings]] - the header that carries seed, config and versions.
- [[cc-randomness-on-the-tick]] - how clojurecraft keeps its randomness replayable.
