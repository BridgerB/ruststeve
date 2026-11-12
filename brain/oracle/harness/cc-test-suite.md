---
title: clojurecraft's tests and checks
description: What clojurecraft tests and statically checks, which workflows run it, and which tests are the best sources of differential cases for ruststeve.
type: reference
tags: [oracle, tests, ci, properties]
aliases: [unit tests, props_test, recordings_test, issues_test, bb check, workflows, probe.yml]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:bb.edn#test {:doc \"Every test, in parallel; args select: -n ns, -v ns/var, -x ns\""
  - "clojurecraft:bb.edn#check {:doc \"Every static check CI runs, in its order\""
  - "clojurecraft:docs/RICH-HICKEY.md#Spec the boundaries, then generate: every packet map is specced from the packet table;"
  - "clojurecraft:src/clojurecraft/spec.clj#Load this namespace and `instrument` in tests; nothing here runs in the hot loop."
related:
  - "[[oracle/harness/_moc|Harness]]"
  - "[[testing-kinds]]"
  - "[[cc-sim]]"
---

# clojurecraft's tests and checks

## Key files
- `test/clojurecraft/*_test.clj`: about 100 test namespaces, one per source namespace and more (`props_test`, `recordings_test`, `issues_test`, `problems_test`, `style_test`, `docs_test`, `model_test`, `scratch_test`).
- `test/recordings/`, `test/issues/`: recorded runs and the snapshot corpus.
- `bb.edn`: `bb test` (every test in parallel), `bb check` (every static check CI runs: format, clj-kondo, Splint, unused vars, the brain checker).
- `.github/workflows/`: `unit.yml`/`test.yml` (tests and checks), `gym.yml`, `race.yml`, `simrace.yml`, `probe.yml` (any question that needs the code run), `fleet.yml`, `world.yml`, `simworld.yml`, `issue.yml`, `pages.yml`.

## How it works
- **Specs** at the boundaries: every packet map, event, world attribute and intent; the reducers (the server model's included) are `fdef`'d and instrumented in tests.
- **Properties**: every keep-alive answered exactly once; the phase only moves along the transition table; `step` total over decodable packets; no FINISH before its deadline; the planner never selects a goal whose needs are unmet. The whole bot runs against the model in 2,000 generated worlds.
- **Style held by tests**: docstrings on every public function, `cond` over nested `if`, every attribute listed in `model.clj`.
- **Problems**: `problems_test` requires a problem statement for every target goal.

## For ruststeve
- The property list above is ready-made for `proptest` ([[testing-kinds]]).
- The pure-function tests (`dig_test`, `recipe_test`, `path_test`, `physics_test`, `chunk_test`, `bytes_test`, `packet_test`, `fluid_test`, `mob_test`) are the richest source of differential cases: same input, same expected output.

## See also
- [[testing-kinds]] - ruststeve's test kinds.
- [[cc-sim]] - closed-loop tests.
