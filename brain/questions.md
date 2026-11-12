---
title: Question set
type: moc
tags: [brain, questions]
status: verified
lastUpdated: 2026-11-11
---

# Question set

Real questions, the traversal that answers each, and how many notes past the index and MOCs it took. Re-run when an area changes; a question that needs more than about five notes is a missing hub.

| Question | Traversal | Notes |
|---|---|---|
| What is the core's whole API, and what happens inside one step? | [[ruststeve/design/_moc]] → [[sans-io-core]] | 1 |
| Why not use azalea? | [[ruststeve/decisions/_moc]] → [[own-protocol-not-azalea]] | 1 |
| Is this "Rust v2"? What do we call it? | [[just-ruststeve]] | 1 |
| Which types are banned in `bot-core`, and why? | [[determinism-rules]] → [[ordered-collections]] | 2 |
| What do I build first? | [[ruststeve/roadmap/_moc]] → [[port-phases]] | 1 |
| How is a compressed frame laid out? | [[wire/_moc]] → [[wire-framing]] | 1 |
| Why does melee deal no damage? | [[wire-actions]] → [[hist-v1-load-bearing-fixes]] | 2 |
| Why does a chunk decode run past the data in the Nether? | [[wire-chunk-sections]] → [[mc-dimensions]] | 2 |
| Why do crafts fail on a loaded server? | [[wire-container-click]] → [[cc-window-clicks]] | 2 |
| How does clojurecraft decide what to do each tick? | [[oracle/architecture/_moc]] → [[cc-goals-and-intents]] → [[cc-plan-and-make]] | 2 |
| What does each clojurecraft namespace do? | [[oracle/_moc]] → [[cc-namespaces]] | 1 |
| How does a Clojure mechanism become Rust? | [[cc-to-rust-map]] | 1 |
| What lessons go in `LESSONS.md`? | [[cc-lessons]] | 1 |
| What threshold says a task is stuck? | [[cc-stall-and-budgets]] → [[expectations-and-watchdog]] | 2 |
| How do I test a changed bot without a server? | [[replay-limits]] → [[cc-sim]] | 2 |
| What actually kills bots at the blaze spawner? | [[cc-blaze-hunt]] → [[mc-blaze]] | 2 |
| Which metadata flag says a blaze is about to fire? | [[mc-blaze]] → [[wire-entity-metadata]] | 2 |
| When should FINISH be sent after START? | [[game/_moc]] → [[mc-dig-timing]] | 1 |
| How do bots get out of water? | [[mc-water]] → [[cc-physics]] | 2 |
| Why was ruststeve v1 reset? | [[history/_moc]] → [[hist-why-v1-failed]] | 1 |
| Where is each bot stuck right now? | [[hist-walls]] → [[cc-roadmap-stages]] | 2 |
| May the bot read lava buried in rock? | [[hist-no-xray]] | 1 |
| Where does a packet id come from? | [[wire-packet-ids]] → [[cc-datagen]] | 2 |
| What happens when the bot gets stuck in a real run? | [[incident-to-regression]] → [[blaze-debugging-method]] | 2 |
| What do we build first, and when is it done? | [[ruststeve/roadmap/_moc]] → [[first-twenty-issues]] | 1 |
| Where does code run? How many runners? | [[runners-not-the-mac]] | 1 |
| How do I run a command on a runner? | [[ci-on-runners]] | 1 |
| What tools do agents have, and how do I add one? | [[agent-tools-over-mcp]] | 1 |
| What format are recordings? Why not JSON? | [[recording-format]] | 1 |
| Do we use Postgres, SurrealDB, or a database at all? | [[data-out]] | 1 |
| How far did clojurecraft get, and why did it stop? | [[oracle/handoff/_moc]] → [[cc-handoff]] → [[cc-stage18-lessons]] | 2 |
| Should we port the pearl rule table? | [[cc-pearls-rule-table]] | 1 |
| Why did sim pass at 30% while real sat at 3%? | [[cc-stage18-lessons]] → [[mc-mob-facts]] | 2 |
| How do we stop the bot burning pickaxes in loops? | [[progress-invariants]] → [[cc-unmerged-leads]] | 2 |
| When does a sim number count? | [[measurement-rules]] | 1 |
| How is a race lane sized? | [[cc-race-and-gym]] | 1 |

## Gaps
- Resolved: `Tick` versus the ms clock ([[sync-core-and-time]]); physics lives in the core ([[body-boundary]]); the server model is ported into the `sim` crate ([[first-twenty-issues]], issue 6).
- Decided: steve is the front end, reading SurrealDB ([[data-out]]). Open: where the database lives.
- Resolved: the work moved to `main`, and code runs on the runners through CI and the probe ([[ci-on-runners]]).
