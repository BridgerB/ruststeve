---
title: Rule tables
description: How tactical decisions (combat above all) are one table of named, prioritised rules evaluated by one function, so the firing rule is the decision's reason.
type: reference
tags: [rules, tactics, combat]
aliases: [Rule, decide, BLAZE_RULES, rules over conditionals]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55 clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#The pearl hunt grew into a rule table of about 60 rows (pearls-rules.edn) whose interactions nobody could hold in their head."
  - "ruststeve:FUNCTIONAL.md#Express tactical decisions (combat especially) as a rule table evaluated by one function. The rule that fired becomes the decision's Reason, so the log says why."
  - "ruststeve:FUNCTIONAL.md#.max_by_key(|r| r.priority)     // keep priorities unique so ties can't depend on order"
  - "ruststeve:FUNCTIONAL.md#Combat logic in a 300-line function of nested ifs. Thresholds as unnamed literals."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[brain-events]]"
  - "[[cc-pearls-rule-table]]"
  - "[[blaze-debugging-method]]"
---

# Rule tables

Gather decision rules into one inspectable place instead of `if`s spread through the program. A rule is `{ name, priority, when: fn(&Ctx) -> bool, then: fn(&Ctx) -> Intent }`; `decide(ctx, rules)` keeps the matching rules, takes the highest priority, and returns the intent with the rule's name, or `(Hold, "no_rule_matched")`. The name becomes the `Reason` and a `RuleFired` event.

## Key files
- `FUNCTIONAL.md`, sections 3.9 and 16.6.

## How it works
- Priorities must be unique so a tie never depends on order.
- Each rule is one readable line, testable alone against a fixture, and visible firing in the recording.
- Thresholds are named `Config` fields, never literals inside `when`.

## Gotchas
- clojurecraft's pearl hunt used this shape and grew to about 60 to 70 rows whose interactions nobody could hold in their head; a fix to one row moved deaths to another. The handoff says not to port that table ([[cc-pearls-rule-table]]). Keep each table small enough to read whole, and pair it with progress invariants ([[progress-invariants]]).

## See also
- [[brain-events]] - `RuleFired`, `CombatFrame`.
- [[blaze-debugging-method]] - the rules the blaze fight might grow.
