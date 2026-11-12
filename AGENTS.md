# AGENTS.md

ruststeve is an autonomous Minecraft bot in Rust: from a fresh spawn to a dead Ender Dragon on vanilla 26.1.2 (protocol 775), with no human input and no cheats. It is a from-scratch port of clojurecraft (the Clojure bot), built in the order of twenty planned GitHub issues. The project's knowledge lives in `brain/`; the plan lives in the issues. This file tells you how to pick up the work at any point, whether we are on issue 1 or issue 17.

## 1. Start every session here

Do these in order before writing any code, every session, and again after any context reset.

1. **Get the branch.** All work is on `main` (the old `functional` and `reset` branches were fast-forwarded into it on 2026-11-11). `git fetch origin && git checkout main && git pull`.
2. **Read the brain's spine, in full.** These are short; read every one:
   - `brain/_index.md`, `brain/operating-manual.md`, `brain/conventions.md`, `brain/rules.md`, `brain/glossary.md`
   - every hub: `brain/*/_moc.md` and `brain/*/*/_moc.md`
   - the handoff, all four notes in `brain/oracle/handoff/` (`cc-handoff`, `cc-stage18-lessons`, `cc-unmerged-leads`, `cc-loop-rules`)
   - every decision in `brain/ruststeve/decisions/`
   - `brain/ruststeve/roadmap/first-twenty-issues.md`
3. **Find where we are.** `gh issue list -R BridgerB/ruststeve --state all --limit 50`. The plan is issues **#6 to #25**, titled `01.` to `20.`. **The current issue is the lowest-numbered open one.** Everything before it is done.
4. **Understand everything before it.** For each closed plan issue, read its body and the PR that closed it (`gh issue view <n> -R BridgerB/ruststeve --comments`, then `gh pr view <pr>`). Read every file in `docs/problems/` if the directory exists. Skim the crates the earlier issues added (`crates/*/src/lib.rs` and the binaries) so you know what exists.
5. **Read the current issue in full**, with its comments, and every note in its "Read first" list. Follow those notes' links only as far as the issue needs (the brain's rule: read, do not dump).
6. **See what is in flight.** `gh pr list -R BridgerB/ruststeve --state open`, `gh run list -R BridgerB/ruststeve --limit 20`. Pick up an open branch or PR for the current issue instead of starting over.
7. Then say in two or three lines where we are (current issue, what is done, what is in flight) and start.

## 2. Where code runs

**No code runs on the Mac.** It only edits code. Everything that runs code goes to GitHub Actions runners: **20 at once, 16 GB each**.

- Locally only: editing, `cargo fmt`, `cargo clippy`, `cargo run -q -p brain-check` (seconds; it is the brain's static check), `git`, `gh`, reading files.
- Not locally: tests, builds for running, sims, races, replays, benchmarks, probes.
- The one exception (Bridger, 2026-11-12): to **watch the bot live**, the bot and a vanilla server may run on the Mac, streaming to steve running locally (`STEVE_RELAY=ws://localhost:8787`). Runners never stream to steve.
- The loop: change code, commit, push the branch, let CI run on the push, read the run's logs (`gh run view <id> --log-failed`), push again.
- A question that needs code run goes to the probe: the `probe_run` MCP tool, or `gh workflow run probe.yml --ref <branch> -f command=...` ([`brain/ruststeve/design/ci-on-runners.md`](brain/ruststeve/design/ci-on-runners.md)).
- **Use the `ruststeve` MCP server** (`.mcp.json`, built with the official Rust MCP SDK `rmcp`): `probe_run`, `probe_output`, `run_status`, `run_log`, `ci_runs`, `brain_check`, `brain_note`, `brain_search`. New agent-facing tooling is added there as a tool, not as a script (`brain/ruststeve/decisions/agent-tools-over-mcp.md`).
- Watch runs in the background and keep working; never end a turn just waiting on a run.

## 3. How to work an issue

- **One issue at a time**, in order. Do not start the next until Bridger says "do the next one".
- **No approval gate** (Bridger, 2026-11-12): on "do the next one", post the plan as a comment on the issue, then implement, open the PR and merge it when green, without stopping to ask. No plan-mode approval dialog for ruststeve issues.
- Branch from `main` as `issue-NN-<slug>` (e.g. `issue-03-connect`). Committing, pushing the branch, opening its PR **into `main`** and merging it are authorized. Never push directly to `main`, never force-push, never `--no-verify`.
- An issue closes only when **its done-condition holds on a runner**, measured as the issue states (real race, k/n, the named checkpoint start). Put the evidence (run ids, k/n, the commit) in the PR body, then merge into `main` and report.
- **A failure inside an issue** gets a written problem statement in `docs/problems/<stage>.md` (needs, risks, done in world terms, the last recorded failure). Never open a new issue for a symptom. An issue too big splits into numbered sub-issues (`17a`, `17b`) so the plan stays one list.
- Every change that touches a subsystem updates its brain note in the same PR, bumping `lastUpdated` and the `ruststeve@` pin; `cargo run -q -p brain-check -- touches ruststeve <paths>` lists the notes a diff affects.

## 4. When the bot is stuck

clojurecraft lost four weeks on its pearl stage, and most of its mistakes are just as easy in Rust (`brain/oracle/handoff/cc-stage18-lessons.md`). Before any behaviour fix:

1. **Throughput first.** How many attempts did the stuck stage get on this commit, real and sim? Under a few hundred, fix the cap (a checkpoint start, lanes) before the bot.
2. **Coverage.** Every ended run must have a case file; under 1.0 is a blind spot fixed first.
3. **Read the evidence yourself.** Rank the case signatures; read three case files of the top one; then replay the recording **at its own commit** and run `why` at the failing moment. A reason code is where a run ended, never why. A cause is a hypothesis until one bot's log lines confirm it.
4. **Name the structural cause** in `docs/problems/`, then fix it. Prefer structure (a progress invariant, a held target, a kept reserve) over a threshold.
5. **Judge the fix** on tens of paired runs (same worlds, same seeds), count the new code path's firings, and confirm a passing stage did not drop.
6. **Sim numbers count only after the parity table** says sim and real agree (`brain/ruststeve/decisions/measurement-rules.md`). The real race is the metric.

Use SurrealDB (the `ingest` binary's insight store) for questions across many runs; use the recording and `why` for one run.

## 5. Commands

```bash
nix develop                                       # dev shell: JDK 25 + rustup; installs the git hooks (flake.nix)
prek run --all-files                              # every hook by hand: fmt, clippy, brain-check, actionlint, nixfmt, secrets
cargo fmt --all                                   # format (local)
cargo clippy --workspace --all-targets -- -D warnings   # lint (local)
cargo run -q -p brain-check                       # the brain's four invariants, must be zero (local)
cargo run -q -p brain-check -- drift              # notes whose cited sources changed since their pin
cargo run -q -p brain-check -- touches ruststeve <paths>   # notes a change affects
gh run list -R BridgerB/ruststeve --branch <branch>        # CI on the runners
```

Tests, the bot and every binary run in the workflows under `.github/workflows/`, not here.

## 6. Code rules

- **The core is pure.** `data`, `protocol`, `world`, `physics`, `memory`, `plan`, `sim` have no I/O, no clock reads, no unseeded randomness, no `env::var`, no `HashMap`/`HashSet` iteration, no `Rc`/`RefCell`/`Arc<Mutex>`. Time and randomness arrive on the tick event. A `clippy.toml` per core crate enforces it (`brain/ruststeve/design/determinism-rules.md`).
- **One owner.** The loop thread owns the world; `step` is the only way it changes; observers read snapshots.
- **Closed sets are enums**, matched with no `_ =>` arm. Kinds generated from the game carry `Unknown(u32)`.
- **Data, not literals.** Thresholds, goals, starts and problem statements are data files; every number has a comment saying where it came from.
- **Our own protocol layer.** No azalea, no sidecar (`brain/ruststeve/decisions/own-protocol-not-azalea.md`).
- **Recordings** are MessagePack with named fields, zstd, commit-pinned; never JSON (`brain/ruststeve/decisions/recording-format.md`). Grow formats by accretion; never rename a field or variant.
- **SurrealDB** only in `ingest`, as the insight store built from result files; never in the core or the recording format (`brain/ruststeve/decisions/data-out.md`).
- **Progress invariants** in every long task: fail fast when it stops getting closer, and count resources spent per unit of progress (`brain/ruststeve/design/progress-invariants.md`).
- **No X-ray**: act only on blocks seen with an exposed face.
- **Never a death as a plan; never fight under danger health.**
- **Rust only** in this repository: every tool and script is a crate or a binary; no `.ts`, `.js` or Python.
- No `unwrap`, `expect` or `panic` in the core; errors are values (`brain/ruststeve/design/error-kinds.md`).

## 7. Commits and PRs

- Conventional prefixes (`feat`, `fix`, `refactor`, `docs`, `chore`, `test`, `ci`), a short imperative subject, a bulleted body wrapped at about 72 columns.
- **No AI attribution anywhere**: no `Co-Authored-By`, no session links, no "generated with" lines, in commits, PR bodies, issues, comments or code. Check before every commit and PR. The `commit-msg` hook (`xtask commit-msg`) refuses it mechanically.
- PR bodies: what changed, the done-condition's evidence (run ids, k/n, commit), what is unverified. Prose is not hard-wrapped.

## 8. Other repositories

- **clojurecraft** (`BridgerB/clojurecraft`): the oracle. Read its code, `brain/`, `LOOP.md` and `docs/HANDOFF-RUSTSTEVE.md`; never change it. Its workflows are disabled. The brain pins it in `brain/sources`.
- **steve** (`BridgerB/steve`, SvelteKit): the front end only, never its TypeScript bot. It reads ruststeve's insight store. Use pnpm there.
- **ruststeve v1**: this repository's `main` at 9e1567f, read by commit for its lessons; its code is not ported.
- Never touch the OCI game box (144.24.32.76), never wipe a world you do not own, never restart a shared server.

## 9. Working with Bridger

- Lead with the outcome, briefly; times in Denver time (America/Denver).
- A missing credential, sign-in, API key or a 401/403: stop and ask; never work around it.
- Ask before anything irreversible outside the repository (money, accounts, the game box, deleting data).
