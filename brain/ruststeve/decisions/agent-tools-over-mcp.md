---
title: Agent tools over MCP
description: Why ruststeve's tools for coding agents (runner probes, CI runs, the brain) are an MCP server built with the official Rust SDK, rmcp, and how an agent uses it.
type: decision
tags: [decision, mcp, tooling, agents]
aliases: [rmcp, MCP server, ruststeve-mcp, .mcp.json, model context protocol, agent tools]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@c5567a0
sourceRefs:
  - "ruststeve:crates/mcp/src/main.rs#//! ruststeve-mcp: an MCP server (the official Rust SDK, rmcp) that gives coding agents"
  - "ruststeve:crates/mcp/Cargo.toml#rmcp = { version = \"=3.9.0\", features = [\"server\", \"transport-io\"] }"
  - "ruststeve:.mcp.json#\"args\": [\"run\", \"-q\", \"--release\", \"-p\", \"ruststeve-mcp\"]"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[ci-on-runners]]"
  - "[[runners-not-the-mac]]"
---

# Agent tools over MCP

## The choice
ruststeve's tools for coding agents are one MCP server, `ruststeve-mcp` (crate `crates/mcp`), built on the official Rust MCP SDK, `rmcp` (pinned at 3.9.0, stdio transport). It is registered for Claude Code in `.mcp.json` at the repository root. Bridger, 2026-11-12: use github.com/modelcontextprotocol/rust-sdk "from now on".

Tools today:

| Tool | Does |
|---|---|
| `probe_run` | dispatch the probe workflow: one command on a runner, for a branch |
| `probe_output` | download a probe's artifact and return `probe.log` |
| `run_status`, `run_log` | a run's jobs and timings; its failed steps' log lines |
| `ci_runs` | recent CI and probe runs, optionally for one branch |
| `brain_check` | the brain checker: invariants, `drift`, or `touches` |
| `brain_note`, `brain_search` | read a note by stem or hub path; find notes by title, description, aliases |

New agent-facing tooling is added here as a tool, not as a script: the insight store's queries, a recording's `why`, the race's status, as their issues arrive.

## Why
- One typed, discoverable interface that any MCP-capable agent can call, instead of each agent re-deriving `gh` and `cargo` invocations.
- It keeps the runner rule mechanical: the tools dispatch work to the runners and read results back; none runs the bot or a test on the Mac ([[runners-not-the-mac]]).
- Rust only, like every tool in the repository ([[just-ruststeve]]).

## Boundaries
- Shell-side tooling only. `rmcp` brings tokio; the core crates never depend on `ruststeve-mcp` (the `deps-boundary` check forbids `rmcp` in them, [[ci-on-runners]]).
- The server runs locally as the agent's tool; it only shells out to `gh`, to `cargo run -p brain-check` and reads files.

## What would change the answer
Bridger only.

## See also
- [[ci-on-runners]] - what `probe_run` dispatches.
