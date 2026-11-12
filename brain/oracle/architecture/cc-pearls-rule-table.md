---
title: The pearl hunt's rule table
description: How clojurecraft's most complex behaviour (hunting endermen for pearls) is a data table of 70 rules over named facts, read by one small function, and why it is the template for ruststeve's rule tables.
type: reference
tags: [oracle, rules, pearls, endermen]
aliases: [pearls-rules.edn, rule table, facts, decide-in, hunt phases, pearl hunt]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Do not port the 60-row pearl rule table or the stage-18 patch pile. Re-derive the hunt from a written problem statement with progress invariants."
  - "clojurecraft:src/clojurecraft/pearls/rules.clj#\"The pearl hunt's rule table (pearls-rules.edn) and the function that reads it: the first row"
  - "clojurecraft:resources/clojurecraft/pearls-rules.edn#;; The pearl hunt's rules, in the order they are tried (clojurecraft.pearls.rules/decide): the first"
  - "clojurecraft:src/clojurecraft/pearls/facts.clj#What the pearl hunt's rules ask of the world: a table from a fact key to a function of the fact getter and the world."
  - "clojurecraft:src/clojurecraft/pearls.clj#Ender pearls by hunting endermen (issue #263), never by bartering: bartering takes a stack of gold the run does not have."
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[rule-tables]]"
  - "[[mc-endermen]]"
---

# The pearl hunt's rule table

The pearl hunt is clojurecraft's rule table in practice: 70 rows in `pearls-rules.edn`, tried in order, each `{:rule/id :rule/phase :rule/when [fact-keys] :rule/then [...]}`. The first row whose facts all hold chooses the intent.

## Key files
- `resources/clojurecraft/pearls-rules.edn`: phases and rules.
- `src/clojurecraft/pearls/rules.clj`: `holds?`, `then`, `decide-in`, `phase`.
- `src/clojurecraft/pearls/facts.clj`: the fact table; each fact computed at most once per call, only when a rule names it.
- `src/clojurecraft/pearls/{sense,move,place,cover,upkeep,params}.clj` and `pearls.edn` (thresholds).

## How it works
- `:rule/when` is a vector of fact keys, each truthy, or `[:not k]` falsy. `:rule/then` is `[:fact k]` (the fact's value is the intent) or `[:intent m]` (a literal with `[:fact k]` holes).
- **Phases** in order: `:leave-fortress`, `:reach-forest`, `:set-up`, `:provoke`, `:fight`; each says which facts mean it is done, and the current phase is the first not done. Rows tagged `:fire`, `:survive`, `:collect`, `:upkeep` apply in every phase.
- The row's `:rule/id` labels the intent (`intent/ranked`); goal ranks and recordings name it, so ids never change.
- The strategy: pearls by killing endermen in a warped forest (a kill drops a pearl one time in two, so twelve pearls are about 24 kills), never bartering. Fight from a burrow an enderman cannot reach into ([[mc-endermen]]).

## For ruststeve: do not port this table
The handoff is explicit: do not port the pearl table (70 rules at the pin; the handoff counts about 60) or the stage-18 patch pile; re-derive the hunt from a written problem statement with progress invariants ([[cc-stage18-lessons]], [[progress-invariants]]). By the end nobody could hold the rows' interactions in their head, and a fix to one row moved deaths to another. What carries over is the *shape* (rules over named facts, the firing rule as the reason, [[rule-tables]]) and the vanilla facts it encoded ([[mc-endermen]], [[mc-mob-facts]]), not the rows. Keep any ruststeve table small enough to read whole.

## See also
- [[rule-tables]] - the Rust shape.
- [[cc-blaze-hunt]] - the stage before, not yet a table.
