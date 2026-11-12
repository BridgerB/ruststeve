---
title: Decisions
type: moc
tags: [moc, decisions]
status: verified
lastUpdated: 2026-11-11
related:
  - "[[ruststeve/_moc|ruststeve]]"
---

# Decisions

Each says what was chosen, what was rejected with the reason, and what would change the answer.

- [[just-ruststeve]] - the name is ruststeve (not v2), a clean reset, Rust-only tooling.
- [[runners-not-the-mac]] - all code runs on GitHub Actions runners (20 at once, 16 GB each); the Mac only edits.
- [[agent-tools-over-mcp]] - agent tools (probes, CI runs, the brain) are one MCP server on the official Rust SDK, rmcp.
- [[issues-are-a-plan]] - twenty planned issues, never roadmap or auto-filed issues.
- [[measurement-rules]] - the real race is the metric; parity before sim numbers; checkpoint starts; caps in game ticks.
- [[recording-format]] - MessagePack with named fields, zstd, commit-pinned; not JSON.
- [[data-out]] - files are the truth; an embedded SurrealDB built from them for insight; steve is the front end; never in the core.
- [[sync-core-and-time]] - threads and bounded channels; the clock and a seed on every tick.
- [[clojure-is-the-oracle]] - port clojurecraft, not v1 or steve; it sets every bar until passed.
- [[own-protocol-not-azalea]] - we parse the byte stream ourselves; no azalea, no sidecar.
- [[one-owner-no-shared-mutation]] - one owner and `&mut`; never `Rc<RefCell>` or `Arc<Mutex>` in the core.
- [[ordered-collections]] - `BTreeMap`/`IndexMap`, never `HashMap`, in the core.
- [[enums-for-closed-sets]] - exhaustive enums for what we own; traits only for open sets.

## See also
- [[cc-decisions]] - clojurecraft's own decisions, the ones ruststeve inherits.
