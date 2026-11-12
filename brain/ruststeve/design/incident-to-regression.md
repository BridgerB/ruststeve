---
title: From incident to regression test
description: The six steps every stuck point goes through, from the flight-recorder dump to a permanent regression recording and a closed-loop success rate.
type: how-to
tags: [incident, regression, flight-recorder, debugging]
aliases: [flight recorder, INCIDENT.md, regression recording, diagnosis]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Every stuck point goes through the same pipeline, so every bug ends up permanently guarded:"
  - "ruststeve:FUNCTIONAL.md#It keeps the last N ticks of snapshots in a VecDeque (start with 600 ticks, 30 seconds of game time)."
  - "ruststeve:FUNCTIONAL.md#Cut the window into recordings/regressions/<name>.jsonl and assert a property of the decision at the incident tick, not an exact output."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[expectations-and-watchdog]]"
  - "[[blaze-debugging-method]]"
---

# From incident to regression test

Every stuck point takes the same path, so every bug ends up permanently guarded.

## Key files
- `FUNCTIONAL.md`, sections 8.5, 9.5 and 16.5.

## Steps
1. **Capture.** A trigger ([[expectations-and-watchdog]]) makes the flight recorder, a `VecDeque` of the last 600 ticks of full snapshots, write `incidents/<tick>-<reason>/`: a case file (trigger, task stack and its why, the last 20 `BrainEvent`s, a ready `replay` command), the moment at the start of the window, and the recording path and tick range ([[observability]]).
2. **Reproduce exactly.** `replay run --from-checkpoint <tick> --until <tick>` must produce the same commands and events. If not, fix determinism first.
3. **Diagnose in writing** in `docs/diagnoses/`, naming the misconception: a belief was wrong, a rule was wrong given correct beliefs, or the action was not carried out as intended (template in [[blaze-debugging-method]]).
4. **Write the open-loop test.** Cut the window into `recordings/regressions/<name>` (a cut of the binary recording) and assert a *property* of the decision at the incident tick (e.g. "while burning, some command moves"), not an exact output.
5. **Fix, then prove it closed-loop**: a scenario's success rate before and after.
6. **Keep the recording** in the suite; upcast it if the schema grows.

## Gotchas
- An exact-output assertion would break on every unrelated improvement; the property is what the bug violated.

## See also
- [[replay-limits]] - why step 5 cannot be skipped.
- [[recordings]] - the format being cut.
