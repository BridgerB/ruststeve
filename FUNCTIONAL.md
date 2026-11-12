# Clojure → Rust Porting Guide: The Minecraft Bot, Done the Rust Way

Nov 11, 2026 · @Bridger

## 0. How to use this guide

You are porting a Minecraft bot from Clojure to Rust. The bot runs from world spawn to killing the Ender Dragon, and it still gets stuck, most often fighting blazes in the Nether. Do not translate it line by line. Rebuild it the way an experienced Rust engineer would, and keep the architecture the Clojure rewrite found: plain values, a time model that records every change, pure decisions, and recorded runs you can replay.

This bot has been written three times: TypeScript first, then a Rust port that copied the TypeScript design, then Clojure. The Rust port failed because it was TypeScript written in Rust syntax. This rewrite must not be Clojure written in Rust syntax either.

### Mission

1. Ship a Rust workspace that plays at least as far as the Clojure bot does today, then further.
2. Make every run produce a record. Any stuck point must be reproducible offline, deterministically, as a failing test, within minutes of the failure.
3. Make every decision explainable. When the bot does something, a human can ask "why?" and get an answer from the logs without attaching a debugger.

### Ground rules

1. **Inventory before you write.** Read the whole Clojure codebase first. Fill in the inventory table from section 15 (namespaces, the state each one owns, the side effects each one performs) before writing any Rust.
2. **Port behavior, not syntax.** Map Clojure namespaces to Rust modules by responsibility, not by name. A 1:1 file mapping is a smell.
3. **The core has no I/O and is deterministic.** No tokio, no wall clock, no OS randomness, no network and no filesystem inside the `bot-core` crate. Time and randomness come in as inputs.
4. **Mutation is fine. Shared mutation is not.** Use `&mut` freely inside one owner. Never use `Rc<RefCell<_>>` or `Arc<Mutex<_>>` in the core. If you think you need one, re-read sections 5 and 13.
5. **Data over objects.** Domain types are plain structs and enums with obvious fields. Behavior lives in functions and `impl` blocks over that data, not in hidden object state.
6. **Closed sets are enums.** Tasks, goals, commands, events and failure reasons are enums, matched exhaustively. Use traits only where the set is truly open (for example, swappable pathfinders).
7. **Every decision carries a reason.** The core emits commands with a reason attached, and the shell logs both as structured data.
8. **No panics in the core.** No `unwrap()` or `expect()` unless the invariant is proven locally and commented. Errors are values (section 10).
9. **Test as you port.** Each ported Clojure function gets a unit test. Each recorded run becomes a replay test.
10. **Keep the Clojure bot alive as the oracle** until the Rust bot passes it. When you can, feed both the same recorded observations and diff their decisions (section 15, phase 3).
11. **When in doubt, choose the boring, explicit option:** an enum over a trait object, a `match` over a callback, an index over a reference, a loop over a clever iterator chain that needs a comment.

### Non-goals

- **No imitating Clojure.** Don't use persistent data structures everywhere, don't model the domain as `HashMap<String, serde_json::Value>`, and don't write macros that recreate `->` or `cond->`.
- **No imitating TypeScript.** No class hierarchies, no event-emitter callbacks that mutate shared state, no `any`-shaped JSON blobs passed around the core.
- **Not a performance project first.** Correctness, observability and reproducibility come first. Optimize only after profiling (section 12).

### How the guide is organized

| Sections | Purpose | How to use them |
| --- | --- | --- |
| 1 to 3 | Why: history, thesis, the Hickey lessons in Rust form | Read once, fully, before coding |
| 4 to 12 | The design spec: architecture, data, core, shell, observability, replay, errors, tests, performance | Follow as rules; deviate only with a written reason |
| 13 to 14 | Anti-patterns and idiom cheat sheet | Check every module against these |
| 15 | The step-by-step porting procedure | Execute in order; each phase has a definition of done |
| 16 | Worked example: the blaze fight | Use as the template for debugging any stuck point |
| 17 to 18 | Checklists, crates, sources | Use per pull request |

### Shared vocabulary

| Term | Meaning in this codebase |
| --- | --- |
| Core | The `bot-core` crate: pure, deterministic decision logic. No I/O. |
| Shell | Everything that touches the outside world: network, clock, files, threads. |
| Observation | An immutable fact the shell reports to the core ("entity 812 moved to x,y,z at tick 5040"). |
| World model | The core's current belief about the world, built only by folding observations. |
| Command | An intent the core emits for the shell to carry out ("attack entity 812"). Plain data. |
| Tick | One step of the loop: observations in, state updated, commands out. |
| Event log | An append-only record of every observation and command, sufficient to replay a run. |
| Flight recorder | A ring buffer of the last N ticks of detail, dumped to disk when something goes wrong. |
| Snapshot | A serialized world model and brain state at one tick, used to start a replay mid-run. |

## 1. Where we've been, and what each version contributes

Each earlier version taught something, and Rust v2 must keep all of it. TypeScript showed the bot can work. Rust v1 showed that copying a design into a new language changes nothing. Clojure showed that the architecture, not the language, was why the bot was hard to observe and fix.

| Version | What worked | What failed | Carry into Rust v2 |
| --- | --- | --- | --- |
| TypeScript v1 | Fast iteration. The JavaScript Minecraft ecosystem for protocol, physics and pathfinding. Event-driven, responsive control. Got deep into the game. | State spread across callbacks and mutable objects. Hard to see what the bot believed or why it acted. Failures couldn't be reproduced, so the blaze stage was debugged by guesswork. | The game knowledge: strategies, heuristics, timings, thresholds and the order of the speedrun. Keep fast iteration, but get it from replay rather than live retries. |
| Rust v1 (TypeScript-shaped port) | Speed, type safety, a single binary. | Copied the TypeScript object graph, so it ended up as `Arc<Mutex<Bot>>`, callbacks and borrow-checker fights. The same opacity as TypeScript, and slower to change. | Every place the borrow checker fought you marks a design flaw (shared mutable state). Keep any protocol or crate integrations that worked. |
| Clojure | Plain values, one place where state changes, pure decision functions, data-driven tasks, an inspectable runtime, much better observability and reproducibility. | Still gets stuck. Runtime shape errors. Dynamic dispatch hides which cases are unhandled. | The whole architecture and the 10 to 20 lessons (inventory below). The data shapes become Rust types. |
| **Rust v2 (this port)** | **Target:** the Clojure architecture, enforced by the compiler. |  | Exhaustive enums show which cases are unhandled. Ownership enforces the time model. Deterministic replay is the test harness. Performance headroom for pathfinding and simulation. |

### Why Rust v1 failed, in one paragraph

TypeScript lets any object hold a reference to any other and mutate it at any time. A literal port of that design gives Rust a graph of objects that all point at each other and all mutate. The borrow checker refuses this, so the usual escape is `Rc<RefCell<_>>` or `Arc<Mutex<_>>` around everything. That keeps all of TypeScript's problems (hidden shared state, no single timeline of changes) and adds lock contention and runtime borrow panics. **Treat borrow-checker friction as a design signal:** it means two parts of the program are trying to own the same changing thing. The Clojure rewrite fixed this by making one place own state. Rust v2 does the same, and the compiler enforces it.

### The lessons register (agent: fill this in first)

The Clojure rewrite produced roughly 10 to 20 concrete lessons. They live in its code and in the author's head, not in this guide. Before porting, create `docs/LESSONS.md` with one row per lesson, map each to a Rust mechanism using section 3, and ask the author to confirm the list. Start from these rows:

| # | Lesson from the Clojure version | Where it shows in the Clojure code | Rust mechanism (section) | Verified by |
| --- | --- | --- | --- | --- |
| 1 | All bot state lives in one value and changes in one place | e.g. a single atom plus a `swap!` loop | One owned `Brain` struct, `&mut` only inside the tick (3.2, 6) | Test: the core has no interior mutability |
| 2 | Decisions are pure functions of state and input | e.g. `(decide state obs) -> commands` | `fn step(&mut Brain, &Input) -> Vec<Command>`, no I/O (4, 6) | Replay test |
| 3 | Every run can be replayed | e.g. an EDN event log | Event log of observations plus a seed (9) | Golden replay in CI |
| 4 | ... |  |  |  |

## 2. The thesis: same disease, different cure

Hickey and Rust diagnose the same disease, shared mutable state, and prescribe different cures. Clojure removes mutation. Rust removes *sharing while mutating*. Port Hickey's diagnosis, and use Rust's cure.

### Hickey's diagnosis

- **Complecting.** "Simple" means one strand, not braided with others. It is objective. "Easy" means familiar and close at hand. It is subjective. In [Simple Made Easy](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/SimpleMadeEasy.md), state complects value and time, objects complect state, identity and value, and methods complect function and state.
- **The epochal time model.** In [Are We There Yet?](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/AreWeThereYet.md), an identity is a label for a succession of immutable values. Each new value is produced by a pure function of the previous one, and an observer always sees a whole, consistent value. Clojure's atom implements this: `(swap! a f args)` makes `(f vN args)` the next value.
- **Place-oriented programming.** In [The Value of Values](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/ValueOfValuesLong.md), new information overwriting old information in place is a habit from the era of tiny memories. Hickey accepts in-place mutation as a way to *build* a value efficiently, but says place has no role in the *information model*.

### Rust's cure: aliasing XOR mutability

Rust allows many shared references (`&T`) or one exclusive reference (`&mut T`) to a value, never both at once. Rust's contribution was showing that a language need not eliminate mutation; eliminating the *combination* of mutation and sharing is enough ([discussion](https://blog.kodewerx.org/2025/)). Niko Matsakis and Manish Goregaokar point out that shared mutation causes data-race-like bugs even in single-threaded programs, which is exactly what bit the TypeScript bot.

The consequence for this port: **while you hold `&mut brain`, nobody else can observe `brain`.** So mutating it in place is indistinguishable, from the outside, from returning a new value. These two signatures are semantically the same program, and the second avoids a copy:

```rust
// Clojure: (swap! brain step input)
fn step_owned(brain: Brain, input: &Input) -> (Brain, Output);   // value in, value out
fn step(brain: &mut Brain, input: &Input) -> Output;              // exclusive access for one tick
```

The tick loop is the atom. It owns the brain, lends exclusive access for exactly one step, then everyone may look again:

```rust
let mut brain = Brain::new(config);
for input in inputs {
    let output = brain.step(&input);   // &mut: no one can see a half-updated brain
    log.record(&input, &output);       // the timeline: append-only facts
}
```

### Concept map

| Hickey concept | Clojure mechanism | Rust mechanism in this codebase |
| --- | --- | --- |
| Value | Persistent immutable collections | An owned `T`. Memory behind a `&T` is frozen while the borrow lives. |
| Identity | `atom`, `ref`, `agent` | The single owner: the `brain` variable in the tick loop |
| State | Value of an identity at a time | `&Brain` at tick N, or a serialized snapshot of it |
| Process (the function F) | Pure function passed to `swap!` | `Brain::step(&mut self, &Input) -> Output`, with no I/O |
| Perception | `deref` / `@` | A shared borrow `&Brain`, or a published `Arc<WorldSnapshot>` for other threads |
| Time | Successive values via compare-and-swap | The tick counter plus the ordered event log, guaranteed by single ownership |
| Memory (remembering the past) | Keep a reference to an old value | The event log plus periodic snapshots. Use `im` persistent collections only if you need many past versions in memory at once. |
| Facts | Datoms, accretion | `Observation` and `Command` enums appended to a log, never edited |

### Where Rust and Hickey disagree, and what to do

Be honest about the tensions rather than forcing Clojure idioms into Rust.

1. **Closed types vs open maps.** Hickey argues for open maps and against product types that make the container define meaning. Rust is built on nominal structs and enums. **Resolution:** inside the core, where you own every line, closed types win. The compiler finds every place a change ripples to. At the edges (logs, config, saved snapshots, any wire format), keep data open and self-describing with `serde`, tolerate unknown fields, and version by adding, not renaming (lesson 3.13).
2. **`Option` everywhere.** In [Maybe Not](https://neonvagabond.xyz/mirrors/maybe-not.html), Hickey argues optionality belongs to the *context of use*, not the aggregate definition. Rust agrees more than it seems. **Resolution:** don't put `Option` on every field of one giant struct. Make a different type for each context ("a block we've seen" vs "a block we've mined"), or parse once into a type where the field is guaranteed (section 5).
3. **Pattern matching.** Hickey lists switch and pattern matching as complecting *who* with *what* in one closed place. Rust leans on `match`. **Resolution:** use `match` where the set is closed by design (commands, task phases, failure reasons). There, exhaustiveness is the feature: adding a `Phase::Nether` variant makes the compiler list every place that has to handle it. That is exactly the "which cases are unhandled" visibility the blaze bug needed. Use traits where the set is open (section 3.7).
4. **Maps vs structs.** Clojure says "just use maps." Rust says use structs, but keep them flat, public, data-only and serializable. You keep the generic-manipulation benefit through `serde`, `Debug`, `PartialEq` and iterators.

**The one-line rule:** in Rust, mutation is how you build the next value, and ownership is how you guarantee nobody sees it half-built.

## 3. The Hickey lessons, translated into Rust

Every Hickey lesson below has a Rust form that keeps the benefit without imitating Clojure's mechanism. Each entry gives the idea, the Rust way, and the smell that means you've slipped back into TypeScript habits.

### Hickey's toolkit, in Rust

This is the complex-vs-simple table from [Simple Made Easy](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/SimpleMadeEasy.md), with a Rust column added.

| Complex construct | What it complects | Simpler construct (Hickey) | Rust equivalent in this codebase |
| --- | --- | --- | --- |
| State, objects | Value, identity and time | Values | Owned structs and enums. Share with `&T`. |
| Methods | Function and state | Functions | Functions and `impl` blocks over plain data with public fields. No hidden fields. |
| Variables | Value and time | Managed references | One owner (the tick loop) plus `&mut` for one step. `Arc` snapshots for readers. |
| Inheritance, switch | Who and what | Polymorphism à la carte | Traits for open sets. Exhaustive enums for closed sets (3.7). |
| Syntax | Meaning and order | Data | Commands, plans and rules as data |
| Imperative loops | What and how | Set functions | Iterator adaptors |
| Actors | What and who | Queues | Channels between shell stages. The core is not an actor. |
| ORM | Many concerns at once | Declarative data manipulation | World-model queries as plain functions |
| Conditionals | Rules and structure | Rules | Rule tables (3.9) |

### 3.1 Simple is not easy: decomplect

**Idea.** Separate things that change for different reasons. Simplicity is about how many concerns are braided together, not how many parts there are.

**Rust way.** Keep perception (building the world model), decision (the brain) and action (the shell) in separate modules with one-way dependencies. The compiler enforces direction: `bot-core` cannot depend on `bot-shell`.

**Smell.** A struct that holds both world state and a network handle. A method that decides *and* sends a packet. A `Bot` god-object.

### 3.2 Values, not places

**Idea.** A value never changes, so it can be shared, logged, compared, sent and remembered for free.

**Rust way.** Observations, commands, plans and snapshots are owned, immutable-by-convention values. Derive the traits that make them inspectable. Never add setters.

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Observation {
    EntityMoved { id: EntityId, pos: Vec3 },
    EntityRemoved { id: EntityId },
    HealthChanged { health: Health, food: u8 },
    BlockChanged { pos: BlockPos, block: BlockKind },
    InventoryChanged { slot: Slot, stack: Option<ItemStack> },
    // ... one variant per kind of fact the shell can report
}
```

**Smell.** `fn set_target(&mut self, ...)` on a shared object. Data structs with private fields and getters.

### 3.3 Identity, state and time: one owner, one timeline

**Idea.** An identity is a succession of values. Only one process advances it, and observers see whole values.

**Rust way.** The tick loop owns `Brain`. `Brain::step(&mut self, &Input) -> Output` is the only way state advances. Tick N's state is whatever `&brain` shows between steps (section 2).

**Smell.** Two tasks that can both mutate bot state. `Arc<Mutex<Brain>>`. A callback that updates state outside the tick.

### 3.4 Facts accrete: the world model is a fold over the log

**Idea.** You can't update a fact, only add a newer one. Datomic treats the log as the source of truth and indexes as derived ([Deconstructing the Database](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/DeconstructingTheDatabase.md)).

**Rust way.** The event log is append-only. The `WorldModel` is a *derived index*: always rebuildable by folding observations. Any cache inside it must be rebuildable the same way.

```rust
impl WorldModel {
    pub fn apply(&mut self, obs: &Observation) {
        match obs {
            Observation::EntityMoved { id, pos } => self.entities.upsert_pos(*id, *pos),
            Observation::EntityRemoved { id } => self.entities.remove(*id),
            // ... exhaustive: a new Observation variant won't compile until handled here
        }
    }
}

pub fn rebuild<'a>(log: impl IntoIterator<Item = &'a Observation>) -> WorldModel {
    log.into_iter().fold(WorldModel::default(), |mut w, o| { w.apply(o); w })
}
```

**Smell.** The world model gets modified anywhere other than `apply`. State that cannot be reconstructed from the log.

### 3.5 Pure core, effects at the edge

**Idea.** Hickey: state leaks complexity into everything around it, and the only containment is a true functional interface (same input, same output).

**Rust way.** The core is sans-IO (section 4). Time, randomness and observations come in as arguments, and commands go out as return values. Example: `now: Tick` is a field of `Input`, never `Instant::now()`.

**Smell.** `tokio`, `std::time`, `rand::thread_rng`, `std::fs` or `println!` anywhere in `bot-core`.

### 3.6 Data over syntax: commands and plans are data

**Idea.** Prefer plain data you can print, store and transform over code paths you can only execute.

**Rust way.** The core never calls the network. It returns `Vec<Command>`, and every command carries a reason. Plans are `Vec<Task>` values you can log and diff.

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Command { pub id: CommandId, pub action: Action, pub reason: Reason }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    LookAt(Vec3), WalkTo(BlockPos), Jump, Attack(EntityId),
    UseItem { slot: Slot }, PlaceBlock { pos: BlockPos, face: Face },
    Mine(BlockPos), SelectSlot(Slot), StopMoving,
}
```

**Smell.** `shell.send_attack(id)` called from decision logic. Plans stored as boxed closures.

### 3.7 Polymorphism à la carte: enums for closed sets, traits for open ones

**Idea.** Keep data, functions and their connection independent. Clojure protocols and multimethods do this.

**Rust way.** Choose by whether the set is closed.

| Situation | Use | Why |
| --- | --- | --- |
| A closed set you own: tasks, phases, actions, failure reasons | `enum` + `match` | Exhaustiveness: the compiler lists every place to update when a variant is added |
| An open set or swappable strategy: pathfinder, protocol backend, test doubles | `trait` + generics | Extension without editing a central enum |
| A runtime-chosen plugin at the shell edge | `Box<dyn Trait>` | Only where static dispatch is impossible |

**Smell.** A trait with a single implementation "for flexibility." `Box<dyn Task>` hierarchies that recreate TypeScript classes. String-keyed dispatch maps.

### 3.8 Queues decouple; the core is fed by one

**Idea.** Direct calls tie components together in time and place. Queues separate producers from consumers ([The Language of the System](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/LanguageSystem.md), core.async).

**Rust way.** Shell stages talk through bounded `tokio::sync::mpsc` channels, which give backpressure. The core consumes one ordered queue of `Input`s and produces `Output`s. The order of that queue *is* the timeline you log.

**Smell.** Shell components calling each other in cycles. Unbounded channels everywhere. Several tasks feeding the core in nondeterministic order without recording that order.

### 3.9 Rules over scattered conditionals

**Idea.** Gather decision rules into one inspectable place instead of `if` statements spread across the program.

**Rust way.** Express tactical decisions (combat especially) as a rule table evaluated by one function. The rule that fired becomes the decision's `Reason`, so the log says *why*.

```rust
pub struct Rule {
    pub name: &'static str,
    pub priority: u8,
    pub when: fn(&CombatCtx) -> bool,
    pub then: fn(&CombatCtx) -> Intent,
}

pub const BLAZE_RULES: &[Rule] = &[
    Rule { name: "retreat_low_health", priority: 100, when: |c| c.health.get() < 8.0, then: |_| Intent::Retreat },
    Rule { name: "dodge_fireball",    priority: 90,  when: |c| c.incoming_fireball.is_some(), then: |c| Intent::Strafe(c.dodge_dir()) },
    Rule { name: "close_distance",    priority: 50,  when: |c| c.target_dist > 3.0, then: |c| Intent::Approach(c.target) },
    Rule { name: "melee",             priority: 40,  when: |c| c.attack_ready, then: |c| Intent::Attack(c.target) },
];

pub fn decide(ctx: &CombatCtx, rules: &[Rule]) -> (Intent, &'static str) {
    rules.iter()
        .filter(|r| (r.when)(ctx))
        .max_by_key(|r| r.priority)     // keep priorities unique so ties can't depend on order
        .map(|r| ((r.then)(ctx), r.name))
        .unwrap_or((Intent::Hold, "no_rule_matched"))
}
```

**Smell.** Combat logic in a 300-line function of nested `if`s. Thresholds as unnamed literals.

### 3.10 Set functions and transducers become iterators

**Idea.** Describe *what* you want from a collection, independent of how it's traversed. Transducers make transformations reusable across contexts.

**Rust way.** Iterator adaptors are lazy and fuse into one loop at compile time, which is the transducer benefit for free. Use them for transformations, and plain `for` loops when the body has effects or complex early exits.

```rust
let nearest_visible_blaze = world.entities()
    .filter(|e| e.kind == EntityKind::Blaze)
    .filter(|e| world.has_line_of_sight(me.eye_pos(), e.pos))
    .min_by(|a, b| a.pos.dist2(me.pos).total_cmp(&b.pos.dist2(me.pos)));
```

**Smell.** `.collect::<Vec<_>>()` between every step. Clever iterator chains that need a comment to explain.

### 3.11 Information is sparse: optionality depends on context

**Idea.** The world doesn't fill in every field. In [Maybe Not](https://neonvagabond.xyz/mirrors/maybe-not.html), whether a field is required depends on where it is used, not on the aggregate.

**Rust way.** Don't build one `Entity` struct with fifteen `Option` fields. Keep the raw, sparse view at the boundary, then parse into context-specific types where fields are guaranteed: `TrackedBlaze { id, pos, last_shot: Tick }` exists only when we know those facts. Use `#[serde(default)]` and tolerate unknown fields in anything persisted.

**Smell.** `.unwrap()` on an `Option` field "because it's always set by then."

### 3.12 Spec becomes types, parsing and property tests

**Idea.** clojure.spec describes data shapes, validates at boundaries and generates test data.

**Rust way.** Three pieces replace spec. (1) Newtypes and enums describe shapes, and the compiler checks them. (2) Parse, don't validate: convert untrusted input once, at the boundary, into types that can't be invalid ([Comprehensive Rust](https://google.github.io/comprehensive-rust/idiomatic/leveraging-the-type-system/newtype-pattern/parse-don-t-validate.html)). (3) `proptest` replaces spec's generators.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockPos { pub x: i32, pub y: i32, pub z: i32 }      // never mix with Vec3

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tick(pub u64);                                          // game ticks, never milliseconds

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Health(f32);
impl Health {
    pub fn new(v: f32) -> Result<Self, DomainError> {
        (0.0..=20.0).contains(&v).then_some(Self(v)).ok_or(DomainError::HealthOutOfRange(v))
    }
    pub fn get(self) -> f32 { self.0 }
}
```

**Smell.** `f64` for positions, health and time interchangeably. Validation functions called in many places.

### 3.13 Grow by accretion, never break (Spec-ulation)

**Idea.** Change is either growth (provide more, require less) or breakage. Turn breakage into accretion by adding a new name instead of changing an old one ([Spec-ulation](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/Spec_ulation.md)).

**Rust way.** This matters most for the event log, because old recordings are your regression suite. Add variants and fields, never rename or repurpose them. Give the log header a schema version. Use `#[serde(default)]` for new fields. If a meaning must change, add `ObservationV2` and an upcaster rather than editing the old variant.

**Smell.** A recording from last week no longer loads.

### 3.14 The REPL becomes replay, snapshots and live inspection

**Idea.** Hickey ranks *runtime intangibility* as a real problem. A REPL makes the running program inspectable.

**Rust way.** Rust has no practical REPL, so build tangibility in four ways. (1) A `replay` binary that loads a recording and lets you step tick by tick, printing state and decisions. (2) `insta` snapshot tests as recorded REPL sessions: they print a value, you review it once, and it's checked forever. (3) A fast edit loop with `bacon` or `cargo watch`. (4) A live debug view: the `Arc<WorldSnapshot>` from 3.17 exposed through a small local HTTP or JSON endpoint, plus the Rerun viewer for spatial time travel (section 8).

**Smell.** Debugging by adding `println!` and re-running the game.

### 3.15 Hammock time and Design in Practice

**Idea.** Most expensive bugs are misconceptions, not typos. State the problem in writing before solving it. Design in Practice names the phases: describe, diagnose, delimit, direction, design, develop.

**Rust way.** Every stuck point gets a short written diagnosis before any code changes (template in section 16). Observability exists to correct misconceptions: you're checking what the bot *believed* against what was *true*.

### 3.16 Identities have names: IDs, not pointers

**Idea.** Values travel anonymously, while identities need stable names.

**Rust way.** Never store a reference or `Rc` to another entity inside a struct. Store its ID and resolve it each time you need it, handling "gone" explicitly. This is the generational-index lesson from Catherine West's [RustConf 2018 keynote](https://kyren.github.io/2018/09/14/rustconf-talk.html).

```rust
pub struct Target { pub id: EntityId, pub acquired_at: Tick }

match world.entity(target.id) {
    Some(e) => Intent::Attack(e.id),
    None => Intent::TargetLost { id: target.id },   // a first-class outcome, logged
}
```

**Smell.** Lifetimes on domain structs (`struct Plan<'a>`). `Rc<RefCell<Entity>>`.

### 3.17 Perception needs no coordination: publish snapshots

**Idea.** In Datomic, writes are coordinated and reads are free, because readers see immutable values.

**Rust way.** Only the tick loop writes. After each tick it publishes an `Arc<WorldSnapshot>` through a `tokio::sync::watch` channel. The debug server, metrics and visualizer read the latest snapshot without ever locking the brain.

**Smell.** The debug UI holds a lock on bot state.

### 3.18 Speculative application: plan by simulating

**Idea.** Datomic's `with` applies a transaction to a database value locally, without touching the real one.

**Rust way.** Evaluate candidate plans with pure functions over borrowed state: `fn simulate(world: &WorldModel, plan: &Plan) -> Outcome`. If simulation needs mutation, clone the small relevant slice (a local `CombatSim`), not the whole world.

### 3.19 Flow: transform, move, route, remember

**Idea.** Keep these four as separate stages, like a production line.

**Rust way.** The shell is a pipeline: decode packets into `Observation`s (transform), feed the core's queue (move), run the core, encode `Command`s into packets (route), and write everything to the log (remember). Each stage is one function or one task.

### 3.20 Partial failure is normal: expectations with deadlines

**Idea.** Distributed systems fail partially all the time, so they need timeouts, retries and idempotent requests.

**Rust way.** A Minecraft server is a remote system that can ignore you. Every command that should change the world registers an `Expectation` with a deadline. The core checks expectations against later observations. A missed expectation is a logged event and an input to replanning. **This is the main mechanism for detecting "stuck."**

```rust
pub struct Expectation { pub cmd: CommandId, pub expect: Expect, pub deadline: Tick }

pub enum Expect {
    EntityHealthDrops(EntityId),
    BlockBecomes(BlockPos, BlockKind),
    ReachWithin { pos: Vec3, radius: f32 },
    ItemCountAtLeast { item: ItemKind, count: u32 },
}
```

**Smell.** Fire-and-forget commands. "Stuck" discovered by a human watching the game.

## 4. Target architecture

The bot is a deterministic core with no I/O, wrapped by a thin imperative shell. One tick loop owns all bot state. Everything that crosses the boundary is a serializable value, and all of it is recorded. This is the "functional core, imperative shell" pattern, which Rust networking calls *sans-IO*: Firezone, Quinn and str0m write protocol logic as pure state machines driven by an outer event loop ([Firezone](https://firezone.dev/blog/sans-io)).

&#91;embedded content: bot architecture · sans-IO core inside a thin shell\]

Only the tick loop calls `Brain::step`. Everything that enters or leaves the core is a value the recorder writes down, and every other reader sees snapshots, never the live brain.

### The core's whole public API

```rust
/// Everything the core learns in one tick. Built by the shell, recorded verbatim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub tick: Tick,                        // game time: the only clock the core sees
    pub observations: Vec<Observation>,    // facts reported since the previous tick, in arrival order
    pub completions: Vec<Completion>,      // results of off-thread work requested earlier
}

/// Everything the core decides in one tick. Recorded verbatim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub commands: Vec<Command>,            // act now (each carries a Reason)
    pub requests: Vec<WorkRequest>,        // expensive pure work for the shell to run elsewhere
    pub events: Vec<BrainEvent>,           // structured "why" records: plan changes, rule firings, missed expectations
}

pub struct Brain {
    world: WorldModel,                     // derived index: fold of all observations
    plan: Plan,                            // the current goal stack, as data
    expectations: Expectations,            // what we expect the world to do, with deadlines
    rng: rand_chacha::ChaCha8Rng,          // the only randomness, seeded and recorded
    config: Config,
}

impl Brain {
    pub fn new(config: Config, seed: u64) -> Self;
    pub fn step(&mut self, input: &Input) -> Output;   // the only way state advances
    pub fn snapshot(&self) -> WorldSnapshot;           // a value for readers
}
```

Inside `step`, the order is fixed and always the same:

1. Fold `input.observations` into `world` (3.4).
2. Fold `input.completions` into the plan, for example a path result arriving.
3. Check expectations against the new world. Emit `BrainEvent::ExpectationMissed` for any that passed their deadline (3.20).
4. Run the planner: keep, advance or replace the current task (section 6).
5. Run the current task's tactic, such as the combat rules, to produce commands. Register an expectation for each command that should change the world.
6. Return `Output`. Nothing else leaves the core.

### The tick loop (shell side)

```rust
loop {
    let input = collector.next_tick().await?;          // batch observations up to the client tick boundary
    recorder.write(&Record::Input(&input))?;           // record BEFORE stepping, so a panic still leaves the input on disk
    let output = brain.step(&input);
    recorder.write(&Record::Output(&output))?;
    snapshot_tx.send_replace(Arc::new(brain.snapshot()));  // readers never touch `brain`
    for cmd in &output.commands { actuator.execute(cmd).await?; }
    for req in output.requests { workers.submit(req); }     // results come back later as Completions
}
```

**Expensive work (pathfinding).** The pathfinder is a pure function, `find_path(&WorldView, from, goal, budget) -> PathResult`, in its own crate. Start by calling it inline from the core with a node budget per tick, which keeps everything deterministic. Move it to a worker thread only if profiling shows the tick budget (50 ms at 20 ticks per second) is at risk. If you do move it, the result returns as a `Completion` in a later `Input`, and because completions are recorded, replay stays exact.

### Workspace layout

```text
minebot/
  Cargo.toml                  # [workspace]
  crates/
    bot-types/   # pure data: ids, positions, Tick, Observation, Command, Action. Deps: serde only.
    bot-path/    # pathfinding as pure functions over a world view. Deps: bot-types.
    bot-core/    # the Brain: world model, planner, tasks, rules, expectations. Deps: bot-types, bot-path, rand_chacha.
    bot-shell/   # protocol adapter, tick loop, channels, recorder, debug server. Deps: tokio, tracing, protocol crate.
    bot-replay/  # CLI: replay, step, inspect, diff, bisect recordings. Deps: bot-core.
    bot-sim/     # optional, later: headless simplified simulator for scenario tests.
  recordings/    # golden runs used as regression tests (Git LFS)
  docs/          # ARCHITECTURE.md, LESSONS.md, diagnoses/
```

Dependencies flow one way: `bot-types` ← `bot-path` ← `bot-core` ← `bot-shell`, and `bot-replay` depends on `bot-core`, never on `bot-shell`.

### Enforce the boundary mechanically

Don't rely on discipline. Add these on day one:

- **A CI check that the core has no I/O dependencies:** `cargo tree -p bot-core -e normal | grep -E 'tokio|azalea|reqwest|std-fs'` must print nothing.
- **Disallowed methods and types in the core.** Clippy reads a `clippy.toml` found from the crate's directory upward, so put this one in `crates/bot-core/` (and `bot-path/`):

```toml
allow-unwrap-in-tests = true

disallowed-methods = [
  { path = "std::time::Instant::now",    reason = "core is deterministic: use Input.tick" },
  { path = "std::time::SystemTime::now", reason = "core is deterministic: use Input.tick" },
  { path = "std::thread::sleep",         reason = "the core never blocks" },
  { path = "std::env::var",              reason = "config comes from the recording header" },
  { path = "rand::thread_rng",           reason = "use Brain.rng (seeded)" },
  { path = "rand::rng",                  reason = "use Brain.rng (seeded)" },
]
disallowed-types = [
  { path = "std::collections::HashMap", reason = "random iteration order breaks replay: use BTreeMap or IndexMap" },
  { path = "std::collections::HashSet", reason = "random iteration order breaks replay: use BTreeSet or IndexSet" },
  { path = "std::cell::RefCell",        reason = "no interior mutability in the core" },
  { path = "std::rc::Rc",               reason = "use IDs, not shared pointers" },
]
```

- **Crate attributes in `bot-core/src/lib.rs`:** `#![forbid(unsafe_code)]`, `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`, `#![warn(clippy::pedantic)]`, then allow specific pedantic lints with a reason when they're noise.

## 5. Modeling data the Rust way

Model the domain as small, flat, public structs and enums. Give every unit its own newtype, refer to other things by ID, store collections keyed by ID in ordered maps, and parse untrusted data once at the boundary.

### 5.1 Rules for every domain type

1. **Public fields, no getters** on plain data. Behavior goes in functions or `impl` blocks that read those fields.
2. **Derive the inspection traits:** `Clone, Debug, PartialEq, Serialize, Deserialize` on everything that crosses the core boundary or appears in a snapshot. Add `Copy, Eq, Hash, PartialOrd, Ord` to small key types.
3. **A newtype for every unit:** `Tick`, `BlockPos` (i32), `Vec3` (f64), `ChunkPos`, `EntityId`, `Slot`, `Health`, `Degrees`. Conversions are named functions (`BlockPos::containing(Vec3)`, `Vec3::center_of(BlockPos)`), never `as` casts scattered around.
4. **Kinds are enums:** `BlockKind`, `ItemKind`, `EntityKind`, `Dimension`. Generate them from the protocol registry or map from it. Include an `Unknown(u32)` variant so a new server version degrades instead of crashing.
5. **No lifetimes, `Rc`, `RefCell` or trait objects in domain types.** If a struct wants a reference to another entity, it gets an ID instead (3.16).
6. **Pass small `Copy` types by value and large types by `&`.** Don't clone to satisfy the borrow checker; restructure instead (section 12).

### 5.2 The world model's shape

```rust
pub struct WorldModel {
    pub me: SelfState,                                  // position, look, health, food, inventory, effects
    pub entities: BTreeMap<EntityId, EntityRecord>,     // ordered map: deterministic iteration
    pub blocks: ChunkStore,                             // sparse: only chunks the server has sent
    pub dimension: Dimension,
    pub last_tick: Tick,
}

pub struct EntityRecord {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub first_seen: Tick,
    pub last_seen: Tick,
}

pub struct ChunkStore {
    pub chunks: BTreeMap<ChunkPos, Chunk>,
}
pub struct Chunk {
    pub sections: Vec<Option<Box<Section>>>,            // None = all air
}
pub struct Section {
    pub blocks: [BlockKind; 4096],                      // 16x16x16, flat array, index = (y*16 + z)*16 + x
}
```

Flat arrays and ID-keyed maps are the data-oriented layout that Catherine West's keynote arrives at after object graphs fail in Rust ([kyren.github.io](https://kyren.github.io/2018/09/14/rustconf-talk.html)). If the core ever needs an internal graph, such as a goal tree with parent links, store the nodes in a `slotmap::SlotMap` and link them by generational keys, so a stale key fails to resolve instead of pointing at the wrong node.

### 5.3 Context types instead of `Option` soup

The raw record stays sparse, because the server rarely tells you everything. Purpose-built types carry guarantees for the code that uses them. This is how Rust honors Hickey's "optionality belongs to the context" (3.11).

```rust
/// Built fresh each tick for the combat tactic. If any required fact is unknown, there is no BlazeTarget.
pub struct BlazeTarget {
    pub id: EntityId,
    pub pos: Vec3,
    pub dist: f64,
    pub visible: bool,
    pub last_fireball: Option<Tick>,                    // genuinely optional in THIS context
}

impl BlazeTarget {
    pub fn from_world(w: &WorldModel, id: EntityId) -> Option<Self> {
        let e = w.entities.get(&id).filter(|e| e.kind == EntityKind::Blaze)?;
        Some(Self {
            id,
            pos: e.pos,
            dist: e.pos.dist(w.me.eye_pos()),
            visible: w.has_line_of_sight(w.me.eye_pos(), e.pos),
            last_fireball: w.last_projectile_from(id),
        })
    }
}
```

### 5.4 Mapping Clojure data to Rust types

| Clojure shape | Rust shape |
| --- | --- |
| `{:x 1 :y 64 :z 3}` | `BlockPos { x: 1, y: 64, z: 3 }` |
| Keyword enumerations (`:overworld :nether :end`) | `enum Dimension { Overworld, Nether, End }` |
| A map with a `:type` key used for dispatch | An enum with one variant per type, data inside the variant |
| Open, growing sets of keywords (tags, labels) | `enum` with an `Other(String)` variant, or a `BTreeSet<Tag>` |
| `nil` meaning "unknown" | `Option<T>` in the raw record. A context type where it's required. |
| `nil` meaning "failed" | `Result<T, E>` (section 10) |
| Vectors of maps | `Vec<Struct>`. A `BTreeMap<Id, Struct>` if looked up by ID. |
| `atom` holding the world | A field of `Brain`, owned by the tick loop |
| Metadata on values (`^{:source ...}`) | An explicit field (`reason: Reason`, `source: ObsSource`) |
| EDN on disk | JSON Lines via `serde_json` while developing. Postcard plus zstd later if size matters. |

### 5.5 Serialization and log format

- **Start human-readable.** Write recordings as JSON Lines: one `Record` per line, a header line first with the schema version, seed, config hash, git commit and Minecraft version. You can `grep`, `jq` and diff them. Hickey's case for self-describing formats applies here.
- **Switch to compact only when needed.** If files get too big, add a binary format (`postcard` + `zstd`) behind the same `Record` type, plus `bot-replay export --json` to convert back.
- **Keep serde's default (externally tagged) enum representation.** It works for every variant shape and reads clearly: `{"EntityMoved":{"id":812,"pos":[...]}}`.
- **Enable `serde_json`'s `float_roundtrip` feature.** Without it, float parsing can be off by one bit in the last place. A property test written for this guide, which checkpoints a world model through JSON and compares the result to a full fold, failed until the feature was enabled.
- **Floats are deterministic on the same binary and platform.** Rust doesn't enable fast-math. Transcendental functions (`sin`, `atan2`) can differ across operating systems. If you need to replay recordings across platforms, use the `libm` crate for those functions in the core.
- **Reject NaN at the boundary.** A `Vec3` with NaN from a bad packet is a parse error, not a value the core ever sees.

### 5.6 Internal IDs are deterministic

`CommandId`, `TaskId` and `ExpectationId` come from counters inside `Brain`, never from UUIDs or the clock, so a replay produces the same IDs and diffs line up.

## 6. The decision core: phases, tasks and state machines as enums

The brain is three layers of plain data, each an enum you can print: a **phase** (where we are in the speedrun), a **task stack** (what we're doing now), and a **tactic** (what to do this tick, often a rule table). Every transition between them is a logged event with a reason.

### 6.1 Phases: milestones defined by world facts

A phase is complete when a predicate on the world model is true, not when some code path says so. That makes progress checkable from any snapshot.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    EarlyGame,        // wood, stone, food, iron tools: exact steps come from the Clojure bot
    BuildPortal,
    EnterNether,
    FindFortress,
    CollectBlazeRods,
    GetEnderPearls,
    CraftEyes,
    FindStronghold,
    EnterEnd,
    KillDragon,
    Done,
}

impl Phase {
    pub fn is_complete(self, w: &WorldModel, cfg: &Config) -> bool {
        match self {
            Phase::CollectBlazeRods => w.me.inventory.count(ItemKind::BlazeRod) >= cfg.blaze_rods_needed,
            Phase::EnterNether => w.dimension == Dimension::Nether,
            // ... list every phase. This wildcard is a porting placeholder only (6.6).
            _ => todo!("port from Clojure"),
        }
    }
}
```

The phase list above is a placeholder. Take the real sequence, and the real completion conditions, from the Clojure bot.

### 6.2 Tasks: an enum with its state inside

Each task variant holds exactly the state it needs. Matching on the variant gives you that state with no downcasting and no `Option` fields for other tasks.

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Task {
    Goto { goal: GoalArea, path: PathState, started: Tick },
    Mine { target: BlockPos, started: Tick },
    Craft { recipe: RecipeId, count: u32 },
    Explore { strategy: ExploreStrategy, visited: BTreeSet<ChunkPos> },
    FightBlaze(BlazeFight),
    // ...
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus { Running, Done, Failed(Failure) }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Failure {
    NoPath { to: GoalArea },
    TargetLost { id: EntityId },
    NoProgress { metric: ProgressMetric, since: Tick },
    MissingItem { item: ItemKind, need: u32, have: u32 },
    TookDamageTooFast { lost: f32, window_ticks: u32 },
    Died,
}
```

The task step function receives what it needs explicitly and writes commands into an emitter, which assigns `CommandId`s and registers expectations:

```rust
pub fn step_task(task: &mut Task, ctx: &TickCtx, out: &mut Emitter) -> TaskStatus {
    match task {
        Task::Goto { goal, path, started } => goto::step(goal, path, *started, ctx, out),
        Task::Mine { target, started } => mine::step(*target, *started, ctx, out),
        Task::FightBlaze(fight) => combat::blaze::step(fight, ctx, out),
        // ... exhaustive
    }
}
```

**Split borrows by destructuring `self`.** The classic borrow-checker fight is needing `&mut self.plan` while reading `&self.world`. Fields of a struct are disjoint, so destructure:

```rust
impl Brain {
    pub fn step(&mut self, input: &Input) -> Output {
        let Brain { world, plan, expectations, rng, config } = self;   // disjoint borrows
        world.apply_all(&input.observations);
        let ctx = TickCtx { world, tick: input.tick, config };
        let mut out = Emitter::new(input.tick, expectations);
        planner::step(plan, &ctx, rng, &mut out);
        out.finish()
    }
}
```

### 6.3 The planner: a task stack with recovery as data

```rust
pub struct Plan {
    pub phase: Phase,
    pub stack: Vec<Task>,           // top = current. Push subgoals, pop on Done.
    pub watchdog: Watchdog,         // generic stuck detection (6.4)
}
```

When the top task returns `Failed(f)`, the planner calls one pure function, `recover(&f, &plan, &world) -> Recovery`, and logs the failure and the recovery chosen:

```rust
pub enum Recovery {
    Retry { after_ticks: u32 },                 // transient: e.g. a missed swing
    PushSubtask(Task),                          // e.g. NoPath -> Explore, or bridge over lava
    ReplaceWith(Task),                          // e.g. TargetLost -> find the next blaze
    PopAndReplan,                               // the task is pointless now
    Escalate { to_phase_task: Task },           // e.g. repeated deaths at the fortress -> gear up first
}
```

This is Hickey's "rules, not conditionals" applied to failure: all recovery policy is in one place, readable as a table, and every choice is in the log.

### 6.4 Progress metrics and the watchdog

Every task kind defines a numeric progress metric and a patience budget. The watchdog is generic and doesn't know anything about blazes.

| Task | Progress metric (higher is better) | Patience (ticks without improvement) |
| --- | --- | --- |
| `Goto` | Negative remaining path length | Agent: take from Clojure config, or measure |
| `Mine` | Block break progress, then item picked up | Agent: measure |
| `FightBlaze` | Damage dealt + rods collected, minus damage taken | Agent: measure |
| `Explore` | New chunks seen | Agent: measure |

When the best metric hasn't improved within the budget, the watchdog fails the task with `Failure::NoProgress`. That turns "stuck" from something a human notices into a logged, replayable event that dumps the flight recorder (section 8).

### 6.5 Enums vs typestate

Use **enums** for every state machine that changes at runtime based on the world: phases, tasks, fights. Use the **typestate pattern**, which encodes state in a value's compile-time type ([Cliffle](https://cliffle.com/tags/type-systems)), only for API protocols in the shell, where misuse is a programmer error. For example, a `Recorder<NoHeader>` that only becomes a `Recorder<Ready>` after `write_header()`, so you can't write records without a header.

### 6.6 Make missing cases impossible to miss

In `bot-core`, deny `clippy::wildcard_enum_match_arm`. Every `match` on a domain enum must list its variants. When you add `Task::Bridge` or `Failure::Burning`, the compiler shows every place that must decide what to do. That is the visibility the Clojure version lacked, where an unhandled dispatch value silently did nothing. The `_ => todo!()` in 6.1 is the one temporary exception during porting, and CI should fail if any `todo!()` remains at the end of phase 4 (section 15).

## 7. The imperative shell: protocol, threads and channels

The shell is thin and boring. It turns packets into `Observation`s, turns `Command`s into packets, runs expensive pure work off the tick thread, and records everything. It makes no game decisions.

### 7.1 Choose the protocol layer (the "body")

The core doesn't care which option you pick, because it only sees `Input` and `Output`. Pick by what gets you to parity fastest.

| Option | What it is | Strengths | Costs |
| --- | --- | --- | --- |
| **A. Azalea** | Rust crates for Minecraft bots: protocol, physics, pathfinder, built on Bevy ECS. Latest is 0.16.0+mc26.1 (March 2026) ([docs.rs](https://docs.rs/crate/azalea/latest)). | One process, one language. Tracks recent Minecraft versions. | Requires nightly Rust. Its ECS is a second world model: never let your core read it directly. Its pathfinder has a time-based timeout, so treat it as an actuator, not as part of the deterministic core. |
| **B. Sidecar body** | Keep the protocol stack the earlier versions used (for example Mineflayer in Node), as a separate process that speaks JSON Lines over stdio. | Reuses a body you already know reaches the Nether. Matches Hickey's systems-talk ideal: values on the wire between simple processes. | Two processes and a serialization boundary. |

**Decision procedure for the agent:** check what the Clojure bot uses to talk to the server. If it already uses a separate body process or a library with a Node or Java equivalent, choose B first for parity, then consider A later. If you start fresh, choose A. Either way, implement it behind one shell-side trait so the body can be swapped, and a simulator can stand in for it in tests:

```rust
/// Shell-side boundary to the game. An open set (azalea, sidecar, simulator), so a trait is right here (3.7).
pub trait Body {
    /// Wait for the next client tick; return everything decoded since the previous one.
    async fn next_tick(&mut self) -> Result<(Tick, Vec<Observation>), BodyError>;
    /// Start carrying out a command. Must be idempotent and supersedable by command id.
    async fn execute(&mut self, cmd: &Command) -> Result<(), BodyError>;
}
```

Pin the Minecraft server version in config and write it into every recording header. A recording is only valid against the version it was made with.

### 7.2 What the adapter must do

- **Parse, don't validate.** Turn raw protocol data into `Observation`s at this boundary only. Reject NaN positions. Map unknown block, item and entity ids to `Unknown(u32)` variants rather than failing.
- **Report what happened to commands.** The server can silently ignore you. The adapter reports `Observation::CommandRejected { id, why }` when it knows a command failed locally (for example, no path, or the item isn't in the inventory). Everything else is caught by expectations in the core (3.20).
- **Make long-running commands supersedable.** `WalkTo`, `Mine` and similar commands take many ticks. The adapter tracks the current movement and mining command, and a newer command or `StopMoving` cancels the old one. The core never awaits anything.

### 7.3 What runs where

| Component | Runs on | Talks through |
| --- | --- | --- |
| Body adapter (azalea or sidecar I/O) | Tokio tasks; azalea's own ECS schedule | Feeds the collector with an `mpsc` channel |
| Collector, tick loop, `Brain` | One task on a `current_thread` runtime | Owns `Brain`. Nobody else touches it. |
| Recorder | Inline in the tick loop: a `BufWriter` flushed every tick (20 writes per second is trivial) | Called directly, so records are never dropped and order is guaranteed |
| Workers (pathfinding, if moved off-thread) | `rayon` pool or `tokio::task::spawn_blocking` | Results come back through a channel, become `Completion`s in a later `Input` |
| Debug server, Rerun logger, metrics | Tokio tasks | `watch::Receiver<Arc<WorldSnapshot>>` only |

Use `#[tokio::main(flavor = "current_thread")]`. Azalea's docs warn that moving a bot into `tokio::spawn` can let ticks run at unexpected moments, and recommend `spawn_local` or the current-thread runtime. A single-threaded tick loop is also simpler to reason about and to replay.

### 7.4 Time in the shell vs time in the core

- **The core measures time in game ticks only.** Deadlines, cooldowns and patience budgets are all `Tick` arithmetic.
- **The shell may use wall-clock time** for network timeouts, reconnect backoff and pacing, using `tokio::time`. None of it reaches the core except through recorded observations such as `Observation::Disconnected`.

### 7.5 Shutdown and reconnects

- Use a `tokio_util::sync::CancellationToken` to stop all tasks on Ctrl-C. On shutdown, write a final snapshot and a closing record so the recording is complete.
- Keep the default `panic = "unwind"` so `BufWriter` flushes the recording if the core panics. The input that caused the panic is already on disk, because the loop records before stepping (section 4).
- On reconnect, start a new recording segment whose header links to the previous one. The brain can keep running across a reconnect, since it only sees the `Disconnected` and `Connected` observations.

### 7.6 Shell errors

The shell is an application, so `anyhow` (or `eyre`) with context is fine at the top level. Body-specific failures are a `thiserror` enum (`BodyError`) so the reconnect policy can match on them. Section 10 covers the core's errors.

## 8. Observability: make the bot's beliefs visible

Observability here means one thing: at any tick, a human can see what the bot *believed*, what it *decided*, *why*, and what *actually happened*, and compare them. Six mechanisms provide that, each with one job.

| Mechanism | Job | Where it lives | Source of truth? |
| --- | --- | --- | --- |
| Recording (`Input`/`Output` log) | Replay any run exactly | Shell writes it; core defines the types | **Yes** |
| `BrainEvent`s | The "why": decisions as data | Core emits them in `Output`; recorded | Yes (part of the recording) |
| `tracing` logs | Developer diagnostics, filterable | Mostly shell | No |
| Rerun visualization | See the world and decisions in 3D over time | Shell, fed from snapshots and events | No (derived) |
| Live debug endpoint | Inspect the running bot | Shell, reads `watch` snapshots | No (derived) |
| Flight recorder | Capture rich detail around an incident | Shell, triggered by events | No (derived) |

### 8.1 Decisions are data: `BrainEvent`

The core never explains itself through log strings. It returns structured events that are recorded, testable with snapshots, and renderable anywhere.

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BrainEvent {
    PhaseChanged { from: Phase, to: Phase },
    TaskStarted { id: TaskId, task: TaskKind, reason: Reason },
    TaskEnded { id: TaskId, status: TaskStatus, ticks: u64 },
    RuleFired { tactic: TacticKind, rule: Cow<'static, str>, priority: u8 },
    ExpectationMissed { cmd: CommandId, expect: Expect, deadline: Tick },
    WatchdogTripped { task: TaskId, metric: ProgressMetric, best: f64, since: Tick },
    Recovered { failure: Failure, chosen: Recovery },
    /// Per-tick tactical context while fighting: the numbers the rules saw.
    CombatFrame(CombatFrame),
}
```

`CombatFrame` is what makes fights debuggable: target id, distance, visibility, incoming projectiles, own health, attack cooldown, the intent chosen and the rule that chose it. Emit one per tick during combat. It costs little and is exactly the data the blaze investigation needs (section 16).

Using `tracing` inside the core is allowed only at `debug`/`trace` level for breadcrumbs. It must never be the only record of a decision.

### 8.2 `tracing` in the shell

Use one span per tick and structured fields, never interpolated strings. Don't hold a span guard across an `.await`. Use `in_scope` for the synchronous step:

```rust
let output = tracing::info_span!("tick", tick = input.tick.0, phase = ?brain.phase())
    .in_scope(|| brain.step(&input));

for ev in &output.events {
    match ev {
        BrainEvent::ExpectationMissed { cmd, expect, .. } =>
            tracing::warn!(cmd = cmd.0, expect = ?expect, "expectation missed"),
        BrainEvent::WatchdogTripped { task, metric, .. } =>
            tracing::warn!(task = task.0, metric = ?metric, "no progress"),
        other => tracing::debug!(event = ?other),
    }
}
```

Install two layers: compact human output on stderr and JSON lines to a file (`tracing-subscriber` with the `json` and `env-filter` features, `tracing-appender::non_blocking` for the file writer, and keep its guard alive in `main`). Filter with `RUST_LOG`, for example `RUST_LOG=info,bot_shell=debug,azalea=warn`.

### 8.3 Visualize with Rerun

[Rerun](https://github.com/rerun-io/rerun) is a Rust-built SDK and viewer for time-series spatial data, used in robotics, which is what a Minecraft bot is. Its viewer can scrub back and forth in time. Log on a `tick` sequence timeline from a shell task that reads snapshots and events:

```rust
let rec = rerun::RecordingStreamBuilder::new("minebot").spawn()?;   // or .save("run.rrd")
rec.set_time_sequence("tick", snap.tick.0 as i64);
rec.log("world/me",      &rerun::Points3D::new([snap.me.pos.as_f32()]))?;
rec.log("world/blazes",  &rerun::Points3D::new(snap.blaze_positions()))?;
rec.log("world/fireballs", &rerun::Arrows3D::from_vectors(snap.fireball_vels()).with_origins(snap.fireball_positions()))?;
rec.log("plan/path",     &rerun::LineStrips3D::new([snap.current_path_points()]))?;
rec.log("brain/events",  &rerun::TextLog::new(format!("{ev:?}")))?;
rec.log("metrics/health", &rerun::Scalars::new([snap.me.health.get() as f64]))?;
```

The archetype names (`Points3D`, `Arrows3D`, `LineStrips3D`, `TextLog`, `Scalars`) are from Rerun 0.43. Check docs.rs for the version you pin, because Rerun's API has been renaming things between releases. The `bot-replay` binary should be able to produce the same `.rrd` file from a recording, so you can visualize any past run.

### 8.4 Live inspection endpoint

A small `axum` server bound to `127.0.0.1` reads the latest `Arc<WorldSnapshot>` from the `watch` channel:

- `GET /snapshot`: the full snapshot as JSON
- `GET /plan`: phase, task stack, watchdog state
- `GET /events?since=<tick>`: recent `BrainEvent`s from a bounded ring buffer
- `POST /incident`: trigger a flight-recorder dump by hand

This is the stand-in for the Clojure REPL's "look at the atom." It is read-only by design: mutating a running bot from outside would break the timeline.

### 8.5 Flight recorder

The recording already has every `Input` and `Output`. The flight recorder adds the expensive detail you don't want to write every tick: full world snapshots. It keeps the last N ticks of snapshots in a `VecDeque` (start with 600 ticks, 30 seconds of game time). When a trigger fires it writes `incidents/<tick>-<reason>/` containing:

1. `INCIDENT.md`: trigger, phase, task stack, last 20 `BrainEvent`s, and a ready-to-run `bot-replay` command for the window
2. `snapshot_start.json`: the snapshot at the start of the window, so replay can begin there
3. `window.rrd`: the Rerun recording for the window
4. The recording file path and tick range

### 8.6 Stuck detection: triggers for the flight recorder

| Signal | Source | Starting threshold |
| --- | --- | --- |
| No progress | Watchdog (6.4) | Per task kind. Port from Clojure or measure. |
| Expectation miss streak | Expectations (3.20) | Several consecutive misses of the same kind |
| Oscillation | Position history while a `Goto` is active | Net movement under a small radius over a window |
| Damage rate | Health history | Health falling faster than the fight can win |
| Recovery loop | Planner | The same failure and recovery pair repeating |
| Death, disconnect, panic | Observations, shell | Always |

The thresholds are placeholders: take real values from the Clojure version where they exist, otherwise measure them from recordings of successful runs. Put them in `Config`, not in code.

## 9. Reproducibility: determinism, recordings and replay

A run is reproducible when the same config, seed and recorded inputs always produce byte-identical outputs. Determinism is fragile: one `HashMap` iteration or one stray clock read breaks it, so teams that rely on it enforce it with lints and with CI that replays every recording and diffs the result.

### 9.1 Determinism checklist

| Source of nondeterminism | Fix in this codebase | Enforced by |
| --- | --- | --- |
| Wall clock | The core reads `Input.tick` only | `disallowed-methods` (section 4) |
| Randomness | One `ChaCha8Rng` in `Brain`, seeded from the recording header. Enable `rand_chacha`'s serde feature so its state goes into checkpoints. | `disallowed-methods` on `thread_rng` and `rng` |
| `HashMap`/`HashSet` iteration | `BTreeMap`, `BTreeSet`, or `IndexMap` (insertion order). `std`'s `HashMap` uses a random seed per process. | `disallowed-types` |
| Task scheduling and channel races | The core is single-threaded. Completions enter through `Input` in recorded order. | Architecture |
| Sorting ties | Sort with a total key: `total_cmp` for floats, then `EntityId` as tie-breaker | Review checklist |
| Float math across platforms | Replay on the same target. Use `libm` for transcendental functions if cross-platform replay matters. | Header records target triple |
| Environment and config | No `env::var` in the core. The config is part of the header, with its hash. | `disallowed-methods` on `std::env::var` |
| Dependency or game version drift | Commit `Cargo.lock`. The header records git commit, crate version and Minecraft version. | Replay refuses a mismatched header unless `--force` |
| Parallelism inside the core | No `rayon` in the core | `cargo tree` check (section 4) |

**One more source, found while testing this guide's code:** `serde_json` parses floats approximately unless its `float_roundtrip` feature is on. A checkpoint written as JSON and read back can then differ in the last bit of a position, and checkpoint-based replay silently diverges. Enable the feature in every crate that reads recordings (5.5).

### 9.2 Recording format

```rust
#[derive(Serialize, Deserialize)]
pub struct Header {
    pub schema_version: u32,        // bump only by accretion (3.13)
    pub seed: u64,
    pub config: Config,
    pub config_hash: String,
    pub git_commit: String,
    pub minecraft_version: String,
    pub target_triple: String,
    pub previous_segment: Option<String>,   // reconnects chain segments (7.5)
}

#[derive(Serialize, Deserialize)]
pub enum Record {
    Input(Input),
    Output(Output),
    Checkpoint(BrainState),         // full serialized Brain every N ticks, for fast seeking
    Note(String),                   // human annotations, e.g. "operator: bot looks stuck"
}
```

One JSON object per line, header first. Write a `Checkpoint` every 1,200 ticks (one minute of game time) so replay can start near any tick without folding from spawn. `Brain` must therefore be `Serialize + Deserialize`, which is one more reason to keep it plain data.

### 9.3 The determinism check

```rust
pub fn verify(rec: &Recording) -> Result<(), Divergence> {
    let mut brain = Brain::new(rec.header.config.clone(), rec.header.seed);
    for (input, recorded) in rec.steps() {
        let actual = brain.step(input);
        if actual != *recorded {
            return Err(Divergence { tick: input.tick, expected: recorded.clone(), actual });
        }
    }
    Ok(())
}
```

Run it in CI on every recording in `recordings/golden/`, twice. Also check that starting from any checkpoint gives the same outputs as replaying from the start. If `verify` fails on a commit that wasn't meant to change behavior, you've introduced nondeterminism or an unintended behavior change. Both are bugs.

### 9.4 Know the limit of replay: open loop vs closed loop

This is the most important subtlety in the whole guide. **A recording is open-loop.** It contains what the world did in response to the *old* brain's commands. Once you change the brain, its commands differ, and the real world would have responded differently. So the recorded inputs after the first changed decision are no longer a valid test of the new behavior.

Use each tool for what it's good at:

| Tool | Valid for | Not valid for |
| --- | --- | --- |
| Exact replay (`verify`) | Determinism. Reproducing a bug exactly. Inspecting beliefs and decisions at any past tick. | Testing a changed brain |
| Open-loop replay with a new brain | The *first* decision that changes, and the beliefs leading to it. Checking that a fix changes behavior at the right tick and nowhere earlier (`bot-replay diff`). | Anything after the first changed command |
| Closed-loop scenario tests on a real local server | Whether a fix actually works: success rate over many attempts | Exact reproduction (servers aren't deterministic) |
| `bot-sim` (optional, later) | Fast closed-loop tests of one tactic, such as dodging fireballs, with a simplified model | Fidelity to real Minecraft physics |

### 9.5 From incident to regression test

Every stuck point goes through the same pipeline, so every bug ends up permanently guarded:

1. **Capture.** The flight recorder (8.5) writes the incident folder automatically.
2. **Reproduce exactly.** `bot-replay run --from-checkpoint <tick> --until <tick>` must reproduce the same commands and events. If it doesn't, you have a determinism bug: fix that first.
3. **Diagnose in writing.** Use the template in section 16. Name the misconception: which belief was wrong, or which rule was wrong given correct beliefs.
4. **Write the open-loop test.** Cut the window into `recordings/regressions/<name>.jsonl` and assert a *property* of the decision at the incident tick, not an exact output.
5. **Fix, then prove it closed-loop.** Add or extend a scenario test on a local server and record its success rate before and after.
6. **Keep the recording.** It stays in the suite. Accretion, not breakage: if the schema evolves, upcast it rather than deleting it.

```rust
#[test]
fn regression_blaze_hold_while_burning() {
    let rec = Recording::load("recordings/regressions/blaze-hold-while-burning.jsonl").unwrap();
    let start = Tick(51_200);                                   // tick from INCIDENT.md
    let mut brain = Brain::from_state(rec.checkpoint_at_or_before(start).unwrap().clone());
    for (input, _) in rec.steps_between(brain.tick(), start) { brain.step(input); }   // fast-forward

    let (input, _) = rec.step_at(start).unwrap();
    let out = brain.step(input);
    // Property, not exact output: while on fire and taking damage, the bot must not hold still.
    assert!(out.commands.iter().any(|c| c.action.is_movement()), "held still while burning: {:#?}", out);
}
```

Set `allow-unwrap-in-tests = true` in the core's `clippy.toml` so the `unwrap_used` lint doesn't apply to tests.

### 9.6 Closed-loop scenario tests on a local server

Run a pinned-version local server with a fixed world seed and operator permissions for the test harness. A scenario is plain data: setup commands (`/tp`, `/give`, `/effect`, spawning mobs, setting time), the phase and task to start in, a time limit, and a success predicate on the world model. Run each scenario many times and report the success rate. This is the only honest measure of whether the blaze fight is fixed, because real fights vary run to run.

## 10. Errors and failure

Separate four kinds of "something went wrong," because each needs a different representation and response. Most of what the bot experiences as failure is not an error at all: it's a normal game outcome, and it belongs in the domain model.

| Kind | Examples | Representation | Response |
| --- | --- | --- | --- |
| **Domain outcome** | No path, target lost, died, missing item, no progress | `TaskStatus::Failed(Failure)` (6.2) | Planner recovery (6.3), logged as `BrainEvent::Recovered` |
| **Boundary error** | Malformed packet, NaN position, unknown registry id | `Result<Observation, ParseError>` in the adapter (`thiserror`) | Map to an `Unknown` variant or drop it, log it, count it as a metric |
| **Infrastructure error** | Disconnect, server restart, disk full | `BodyError` (`thiserror`), `anyhow` at the top of the shell | Reconnect with backoff, or stop cleanly with the recording flushed |
| **Invariant violation (a bug)** | Negative item count, entity in two chunks, task stack empty mid-phase | `debug_assert!` plus `BrainEvent::InvariantViolated` | Panics in tests. In live runs: an event and an incident dump, then continue or stop by config. |

### 10.1 Rules

1. **Domain failures are values, not `Err`.** `Failure` is an enum with data, and recovery matches on it. Don't use `Result` for "the blaze flew away"; that's a normal outcome the planner must handle.
2. **Typed errors in libraries, `anyhow` in the application.** `bot-types`, `bot-core`, `bot-path` and the adapter define `thiserror` enums whose variants carry the data needed to understand the problem, like Clojure's `ex-info` with `ex-data`. Only `bot-shell`'s `main` and `bot-replay`'s CLI use `anyhow` with `.context(...)`.
3. **Errors are data too.** Derive `Serialize` on error enums that can appear in recordings or events.
4. **No panics in the core.** Deny `unwrap_used`, `expect_used` and `panic` (section 4). Use `?`, `Option` combinators and `let ... else`. Use checked or saturating arithmetic on `Tick` (`tick.saturating_sub(start)`), never bare subtraction that can underflow.
5. **Don't swallow.** Every `Err` that's handled by dropping something produces a log line or a counter. A boundary error that silently disappears is how the bot ends up believing something false.

```rust
#[derive(Debug, thiserror::Error, Serialize)]
pub enum ParseError {
    #[error("non-finite position for entity {id:?}: {raw:?}")]
    NonFinitePosition { id: EntityId, raw: [f64; 3] },
    #[error("unknown entity id {id:?} in {packet}")]
    UnknownEntity { id: EntityId, packet: &'static str },
}
```

### 10.2 If the core panics anyway

Let it crash. The `Input` that caused the panic is already on disk (the loop records before stepping), so the panic is perfectly reproducible with `bot-replay`. A crash with a perfect reproduction beats a bot limping on with corrupted state. If uptime matters later, a supervisor can restart the process from the last checkpoint, but fix the bug first.

## 11. Testing strategy

Because the core is pure and its inputs are plain values, almost everything can be tested without a server, and test data is cheap to fabricate. Hickey lists "easy to fabricate" as a benefit of values. Use seven kinds of test, each for a different question.

| Kind | Question it answers | Tooling | When it runs |
| --- | --- | --- | --- |
| Unit | Does this pure function do what it says? | `#[test]`, `cargo nextest` | Every save (`bacon`) |
| Snapshot | Did this decision or event stream change? | `insta` + `cargo insta review` | Every commit |
| Property | Does an invariant hold for all inputs? | `proptest`, `proptest-state-machine` | Every commit |
| Determinism replay | Same inputs, same outputs? | `bot-replay verify` over `recordings/golden/` | Every commit (CI) |
| Regression replay | Is a past bug's bad decision gone? | Property assertions over `recordings/regressions/` (9.5) | Every commit (CI) |
| Differential (during the port) | Does Rust decide like Clojure? | Shared JSON fixtures run through both | Phase 3 of the port |
| Closed-loop scenario | Does the bot actually succeed? | Local server harness (9.6), success rates | Nightly and before merging tactic changes |

### 11.1 Fixtures: builders over plain data

```rust
let world = WorldBuilder::new()
    .dimension(Dimension::Nether)
    .me_at(Vec3::new(0.5, 70.0, 0.5)).health(14.0)
    .blaze(EntityId(812), Vec3::new(4.0, 72.0, 1.0))
    .fireball_toward_me(Vec3::new(3.0, 72.0, 1.0))
    .solid_floor(70, -8..=8, -8..=8)
    .build();
```

Builders live in a `bot-core::testkit` module behind a `testkit` feature, so `bot-replay` and the scenario harness can reuse them.

### 11.2 Snapshot tests: the recorded REPL session

```rust
#[test]
fn blaze_rules_dodge_when_fireball_incoming() {
    let ctx = CombatCtx::from_world(&fixtures::blaze_with_incoming_fireball(), EntityId(812)).unwrap();
    insta::assert_debug_snapshot!(decide(&ctx, BLAZE_RULES));
}
```

The first run writes the snapshot, you review it with `cargo insta review`, and it's checked forever after. Snapshot the `BrainEvent` stream for short scripted input sequences too: a change in *why* the bot decided is as important as a change in *what*.

### 11.3 Property tests: spec's generators, in Rust

Good invariants for this bot:

- **Checkpoints are exact.** Folding the full log equals restoring a mid-log checkpoint and folding the rest.
- **Serialization round-trips.** `Brain`, `Input`, `Output` and `Record` survive serialize, then deserialize, unchanged.
- **Paths are legal.** Every step returned by `find_path` is a legal move in the given world view, and the path ends inside the goal.
- **Rules respect preconditions.** No `Attack` intent when the target isn't visible or in reach. No `Melee` while health is below the retreat threshold.
- **The world model is consistent.** After any observation sequence, no entity sits in two places and no inventory count is negative.

```rust
proptest! {
    #[test]
    fn checkpoint_then_suffix_equals_full_fold(obs in arb_observations(0..400), cut in 0usize..400) {
        let cut = cut.min(obs.len());
        let full = rebuild(&obs);
        let mid: WorldModel = roundtrip_json(&rebuild(&obs[..cut]));
        let resumed = obs[cut..].iter().fold(mid, |mut w, o| { w.apply(o); w });
        prop_assert_eq!(full, resumed);
    }
}
```

For the world model as a whole, `proptest-state-machine` compares a system under test against a simple reference state machine over generated transition sequences ([Proptest book](https://proptest-rs.github.io/proptest/proptest/state-machine.html)). A `BTreeMap` of entity positions is a fine reference model for the entity tracker.

### 11.4 Differential testing against the Clojure bot (during the port)

The Clojure bot is the oracle until Rust passes it (ground rule 10). For each pure Clojure function you port (phase predicates, damage math, rule selection, path costs):

1. Generate cases in Rust with `proptest` and write them as JSON.
2. Run a small Clojure script that reads the JSON, calls the original function, and writes the results.
3. Compare in a Rust test. Exact equality for math. For decisions, report agreement per rule and investigate every disagreement.

If the Clojure bot can record its inputs and decisions, convert one of its runs to the Rust `Input` format and replay it open-loop through the Rust brain. Divergences at the start of the run, before decisions feed back into the world, are porting bugs.

### 11.5 Housekeeping

- Use `cargo nextest run` for speed and per-test isolation.
- Unit tests go in `#[cfg(test)] mod tests` next to the code. Cross-module tests go in `crates/bot-core/tests/`.
- Keep the full core test suite under about 30 seconds so it can run on every save. Move slow scenario tests behind `--features scenarios` or `#[ignore]`.

## 12. Ownership hygiene and performance

Good Rust ownership and good Hickey design point the same way: one owner, borrow to read, and mutate in one place. When the borrow checker objects, change the data layout, not the safety mechanism.

### 12.1 When the borrow checker says no

Work down this list in order. Stop at the first step that works.

1. **Split the borrow.** Destructure `self` into its fields (6.2), or borrow two different fields directly. Disjoint fields never conflict.
2. **Perceive, then act (two phases).** Compute what to change using `&` borrows, collect it into a small `Vec` of changes, then apply them with `&mut`. This is Hickey's perception-then-process split, and it removes most conflicts.
3. **Use an ID instead of a reference.** Store `EntityId` or a slotmap key and look it up when needed (3.16).
4. **Move out, transform, move back.** For enum state transitions, take ownership temporarily:

```rust
// When the new state needs only Copy data from the old one, an if-let chain (Rust 2024) is enough.
if let Task::Goto { goal, path, .. } = task && path.is_done() {
    *task = Task::Mine { target: goal.block(), started: now };
}

// When the new state must take owned, non-Copy data from the old one, move it out first.
// Task needs a cheap Default variant, e.g. Task::Idle.
let old = std::mem::take(task);
*task = match old {
    Task::FightBlaze(fight) if fight.target_lost() => Task::Explore {
        strategy: ExploreStrategy::NearSpawner,
        visited: fight.into_seen_chunks(),          // moved, not cloned
    },
    // List unchanged variants explicitly: clippy::wildcard_enum_match_arm rejects `other => other`.
    unchanged @ (Task::Idle | Task::Goto { .. } | Task::Mine { .. } | Task::Craft { .. }
        | Task::Explore { .. } | Task::FightBlaze(_)) => unchanged,
};
```

5. **Clone something small.** Cloning a `Copy` position or a short `Vec` is fine. Cloning the world model to get past an error is not.
6. **Restructure ownership.** If none of the above works, two parts of the code really do want to own the same changing thing. Fix the design. In the core, the answer is never `Rc<RefCell<_>>` (section 4 bans it).

### 12.2 Sharing big read-only data: `Arc` and copy-on-write

Readers on other threads (debug server, Rerun task, path workers) need world data without blocking the tick. Use the Rust form of Clojure's structural sharing:

```rust
pub struct ChunkStore { pub chunks: BTreeMap<ChunkPos, Arc<Chunk>> }

// Snapshot: clones the map of Arcs (cheap), not the chunks.
let snap_chunks = world.blocks.chunks.clone();

// Core mutation: Arc::make_mut clones a chunk only if some reader still holds it.
let chunk = Arc::make_mut(world.blocks.chunks.entry(cpos).or_default());
chunk.set(local_pos, block);
```

Readers see an immutable value. The writer pays for a copy only when there's a reader to protect. Reach for the `im` or `rpds` persistent collections only if you need many versions of a large collection alive at once, for example a planner exploring hypothetical worlds.

### 12.3 Performance budget and measurement

- **Budget:** one tick is 50 ms at 20 ticks per second. Aim for `Brain::step` under 5 ms at the 99th percentile, which leaves room for pathfinding and the shell.
- **Measure, don't guess.** Record step duration in the shell (wall-clock is fine there) as a histogram metric. Profile with `cargo flamegraph`. Benchmark the pathfinder with `criterion` or `divan` on recorded world slices.
- **Profiles:** in `Cargo.toml` set `[profile.release] debug = "line-tables-only"` for useful profiles, and `[profile.dev.package."*"] opt-level = 3` so dependencies are fast in debug builds. Azalea's docs recommend the same for development.
- **Pathfinder data layout:** flat per-section arrays, precomputed passability bitsets, `BinaryHeap` with `Reverse` for the open set, and search buffers reused across calls. That last one is internal mutation behind a pure interface (same input, same output), which is fine.
- **Don't micro-optimize `Output`.** A few small `Vec`s per tick at 20 Hz costs nothing. Clarity wins there.

## 13. Anti-patterns: accents to listen for

Rust v1 failed because it spoke Rust with a TypeScript accent. The same can happen with a Java accent or, in this port, a Clojure one. Review every module against these tables.

### 13.1 TypeScript accent (what sank Rust v1)

| Symptom in Rust | Why it hurts | Do instead |
| --- | --- | --- |
| `Arc<Mutex<Bot>>` passed everywhere | Hidden shared mutation, lock contention, no single timeline | One owner: the tick loop (3.3) |
| Event callbacks that mutate state (`on_tick`, `on_packet` closures) | Changes happen outside the timeline and can't be replayed | Callbacks only push into a channel. The tick loop consumes it. |
| `async` in decision logic | Ordering depends on the scheduler, and it's hard to test | Sans-IO core. Only the shell is `async`. |
| `serde_json::Value` passed through the core | Runtime shape errors, the same failure class as `any` | Typed enums and structs, parsed at the boundary |
| Strings for kinds (`"blaze"`, `"minecraft:blaze_rod"`) | Typos compile, and `match` can't be exhaustive | `EntityKind::Blaze`, `ItemKind::BlazeRod` |
| `setTimeout`-style delays with sleeps | Wall-clock time in decisions makes replay impossible | Tick deadlines in the core |
| Exceptions for control flow (panics, `?` on game outcomes) | Game outcomes look like crashes | `TaskStatus::Failed(Failure)` (section 10) |

### 13.2 Java and C++ OOP accent

| Symptom in Rust | Do instead |
| --- | --- |
| `trait Entity { fn update(&mut self, world: &mut World); }` hierarchies | Plain data plus functions over it: "systems" in ECS terms (section 5) |
| Getters and setters on plain data | Public fields |
| `Manager`, `Service`, `Factory`, `Handler` structs with one method | A function |
| Visitor pattern | `match` on an enum |
| Observer pattern with registered listeners | `BrainEvent`s returned as data in `Output` |
| Back-pointers to parents (`Rc<RefCell<Parent>>`, `Weak`) | IDs or slotmap keys |
| Lifetimes on domain structs to avoid copies | Owned data or IDs. Lifetimes belong on short-lived views. |
| A builder for every struct | A struct literal with `..Default::default()`. Builders only for test fixtures. |

### 13.3 Clojure accent (the risk in this port)

The Clojure *ideas* transfer. Many Clojure *mechanisms* are unnecessary in Rust because ownership already gives you what they were for.

| Symptom in Rust | Why it's unnecessary or harmful | Do instead |
| --- | --- | --- |
| `HashMap<String, Value>` as the domain model | Throws away the compiler. Also random iteration order. | Structs and enums. Open data only at the edges (section 2). |
| `im` persistent collections everywhere | Ownership already gives value semantics. Persistent maps are slower for single-owner use. | `Vec` and `BTreeMap`. `Arc` plus `make_mut` for shared reads (12.2). |
| `.clone()` everywhere "to stay immutable" | `&mut` under a single owner is already safe | Mutate in place inside `step` |
| Chains of `fn f(self) -> Self` moving big structs | Equivalent to `&mut self` but noisier | `&mut self` |
| Macros recreating `->`, `->>`, `cond->` | Rust already has method chaining and expressions | Plain code, iterator chains |
| `HashMap<&str, fn(...)>` for multimethod dispatch | Loses exhaustiveness | `match` on an enum. A trait if the set is truly open. |
| `Option<Option<T>>` and nil-punning helpers | Two meanings of "nothing" collapse | Distinct enum variants that say what absence means |
| Laziness everywhere | Iterators are already lazy, and lazy state is hard to inspect | Collect when you store, iterate when you transform |
| Hunting for a REPL (`evcxr`) as the main workflow | Rust's strength is compile-time checks plus fast tests | Replay tool, `insta` snapshots, live snapshot endpoint (3.14) |

### 13.4 General Rust smells to flag in review

- `.clone()` added only to silence a borrow error (see 12.1 instead)
- `unwrap()`, `expect()` or `as` casts in the core
- A `match` with `_ =>` on a domain enum
- A trait with exactly one implementation and no test double
- `#[allow(...)]` without a comment explaining why
- A module over about 800 lines, or a function over about 80 lines, that mixes perception, decision and action

## 14. Clojure → Rust idiom cheat sheet

Use this as a lookup while porting. Where the Rust column says "not needed," ownership already provides what the Clojure construct was for.

| Clojure | Rust in this codebase |
| --- | --- |
| `(def x 1)`, `let` | `let x = 1;` Immutable by default. Shadowing with a second `let x` is idiomatic. |
| `(defn f [x] ...)` | `fn f(x: T) -> U { ... }` |
| `atom` + `swap!` | The single owner plus `fn step(&mut self, ...)` |
| `reset!` | Plain assignment inside the owner |
| `deref`, `@` | `&T` borrow. Across threads: `watch::Receiver::borrow()` on an `Arc<Snapshot>`. |
| `ref` + `dosync` (STM) | Not needed. `&mut` to the owner updates several fields atomically with respect to readers. |
| `agent` | A Tokio task with an `mpsc` mailbox (the actor pattern) |
| `future`, `promise` | `tokio::spawn` + `JoinHandle`, or a `oneshot` channel |
| core.async `chan`, `go`, `>!`, `<!`, `alts!` | `tokio::sync::mpsc`, `tokio::spawn`, `.send().await`, `.recv().await`, `tokio::select!` |
| `->`, `->>` | Method chaining, iterator chaining |
| `cond->` | `if cond { x = f(x); }` steps, or small named functions |
| `some->` | `?` on `Option`, `and_then`, `map` |
| `map`, `filter`, `reduce` | `.iter().map(..).filter(..).fold(..)` |
| Transducers, `comp` of transducers | Iterator adaptor chains. Return `impl Iterator<Item = T>` to reuse a pipeline. |
| `into` | `.collect::<C>()`, `extend` |
| `group-by`, `frequencies` | `fold` into a `BTreeMap<K, Vec<V>>` or `BTreeMap<K, usize>` (ordered, so deterministic) |
| `assoc`, `update`, `dissoc` | Field assignment on a struct. `map.insert`, `map.entry(k).and_modify(..).or_insert(..)`, `map.remove`. |
| `assoc-in`, `update-in` | Nested field paths: `w.me.inventory.slots[i] = stack;` |
| `merge` | `extend`, or struct update syntax `Config { x, ..base }` |
| `select-keys` | A context type built by a function (5.3) |
| Keywords `:nether` | Enum variants: `Dimension::Nether` |
| Namespaced keywords `:bot/phase` | Module path plus type: `bot_core::Phase` |
| `defrecord` | `struct` with derives |
| `defprotocol`, `extend-protocol` | `trait` plus `impl Trait for T` |
| `defmulti`, `defmethod` | An `enum` plus `match`. A trait if the set is open. |
| `case`, `cond`, `condp` | `match` with guards |
| `if-let`, `when-let` | `if let Some(x) = ...`, `let Some(x) = ... else { return ...; };` |
| `nil` | `None`, or a specific enum variant that says what absence means |
| `try`, `catch`, `ex-info`, `ex-data` | `Result<T, E>` with a `thiserror` enum whose variants carry fields |
| `s/def`, `s/valid?`, `s/conform` | Newtypes with constructors returning `Result`. Deserializing with serde is conforming. |
| `s/gen`, `test.check` | `proptest` strategies. `proptest-derive` for `Arbitrary`. |
| `s/fdef` + `instrument` | `debug_assert!` pre/postconditions plus property tests |
| EDN | serde: JSON Lines for recordings, RON for hand-edited config files |
| `pprint`, `tap>` | `{:#?}` Debug output, `insta` snapshots, Rerun, `tracing` |
| REPL `(comment ...)` blocks | Tests run by `bacon`. Small `examples/` binaries for exploration. |
| `with-redefs` (mocking) | Pass dependencies as parameters. A trait for the `Body`. |
| `binding` (dynamic vars) | An explicit context parameter, such as `TickCtx` |
| Integrant, Component, mount | Wiring in `main`, cleanup with RAII (`Drop`), `CancellationToken` for shutdown |
| `memoize` | An explicit cache field keyed by the input, which must be rebuildable |
| Metadata (`^{...}`) | Explicit fields |
| `loop` / `recur`, `doseq` | `loop`, `while`, `for` |
| `lazy-seq`, `iterate` | `impl Iterator`, `std::iter::from_fn`, `std::iter::successors` |
| `comp`, `partial`, `juxt` | Closures, and a tuple of results for `juxt` |
| `rand`, `rand-nth`, `shuffle` | Methods on the brain's seeded `ChaCha8Rng` (in rand 0.9: `rng.random_range(a..b)`, `slice.choose(&mut rng)`) |
| `System/currentTimeMillis` | Shell only. The core uses `Tick`. |

## 15. The porting procedure, phase by phase

Port in eight phases. Each ends at a gate with a concrete definition of done. Build the observability and replay machinery *before* the intelligence, so every behavior you port is debuggable from its first run. Don't start a phase until the previous gate passes.

&#91;embedded content: porting roadmap · 8 phases, each with a gate\]

Phases 0 to 3 build the foundations and the debugging machinery. Phases 4 to 7 add gameplay, with the blaze fight in a phase of its own.

### Phase 0: Inventory and hammock time (no Rust yet)

Read the entire Clojure codebase. Then write three documents:

1. `docs/INVENTORY.md`, one row per namespace:

| Clojure namespace | Responsibility | State it owns or touches | Side effects | Rust destination (crate::module) | Notes |
| --- | --- | --- | --- | --- | --- |
| e.g. `bot.world` | Fold events into the world model | The world atom | None | `bot-core::world` |  |
| e.g. `bot.combat` | Choose combat actions | Reads world | Sends attack packets | `bot-core::tactics::combat` (decide) + shell actuator (send) | Split decide and act |

2. `docs/LESSONS.md`: the lessons register from section 1, confirmed by the author.
3. `docs/PORT_NOTES.md`: every dispatch keyword (each becomes an enum variant), every threshold and magic number (each becomes a `Config` field), how the Clojure bot records and replays runs, which protocol body it uses (decides 7.1), and any code you will *not* port, with the reason.

**Gate:** the author reviews and approves all three.

### Phase 1: Skeleton and guardrails

- Create the workspace and crates from section 4, with the dependency rules.
- Set up CI: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (with the `clippy.toml` from section 4), `cargo nextest run`, the `cargo tree` I/O check, and a `bot-replay verify` job over `recordings/golden/` (empty for now).
- In `bot-types`: `Tick`, positions, ids, `Observation`, `Action`, `Command`, `Input`, `Output`, `BrainEvent`, `Header`, `Record`.
- In `bot-core`: a `Brain` stub whose `step` returns an empty `Output`.
- In `bot-replay`: `load`, `verify`, `print --from <tick> --to <tick>`.

**Gate:** CI is green. A hand-written recording round-trips through serialize and deserialize, and `verify` passes on it.

### Phase 2: Body and recording, end to end (no intelligence)

- Implement the `Body` adapter (7.1) and the tick loop with the recorder.
- Drive the bot with a trivial scripted brain (walk a square, look around, mine one block) to exercise commands.
- Bring up the live snapshot endpoint (8.4) and Rerun logging (8.3).

**Gate:** a 10-minute recorded run on the local server. `verify` passes on it. The world model rebuilt from the recording equals the live snapshot at the end. You can watch the run in Rerun from the recording file alone.

### Phase 3: Port the core, bottom-up and test-first

Port in dependency order, so each layer is tested before anything builds on it:

1. Units, ids and kinds (`bot-types`)
2. `WorldModel::apply` for every observation (the fold)
3. Derived queries: line of sight, nearest entity, inventory counts, reachable blocks
4. Pathfinding: a pure `bot-path`, or adapter-backed `WalkTo` with expectations, per 7.1
5. Tasks: `Goto`, `Mine`, `Craft`, `Explore`, and the rest, one at a time
6. Tactics and rule tables, combat last
7. The planner, recovery table and watchdog
8. Phases and their completion predicates

For each Clojure function: write the Rust signature with types first. If it needs more than about five parameters or any lifetimes, introduce a context struct or rethink the boundary. Then port the body, add unit tests, and add a differential test against Clojure where the function is pure (11.4).

**Gate (per module):** tests green, differential agreement investigated, no `todo!()` left in the module.

### Phase 4: Early-game parity

- Measure the Clojure bot's baseline first: run its early game N times on the pinned server and world seed, and record its success rate and time to each milestone.
- Run the Rust bot from spawn to entering the Nether, with scenario tests per milestone.
- Save successful runs to `recordings/golden/`.

**Gate:** the Rust success rate and milestone times are at least the Clojure baseline over the same N attempts. No `todo!()` anywhere in `bot-core`.

### Phase 5: Nether, fortress and blazes

This is the known sticking point. Follow section 16 for every failure. Build scenario tests for finding the fortress, approaching a spawner, and fighting blazes from several starting states.

**Gate:** a blaze-rod collection success rate the author sets, measured over a scenario batch, plus a regression recording for every distinct failure found along the way.

### Phase 6: Endgame

Ender pearls, eyes of ender, stronghold, portal, the End, and the dragon. Same method: scenarios, recordings, regressions.

**Gate:** full runs from spawn to dragon kill, with a success rate the author sets.

### Phase 7: Hardening

- Hold the performance budget (12.3) under soak tests.
- Add schema upcasters so every old recording still loads.
- Bring `docs/ARCHITECTURE.md` up to date: the crate map, the data flow, and where to look when the bot gets stuck.

### Working rules for every phase

- **Small pull requests:** one module or one task kind per PR, with its tests in the same PR.
- **Update the docs in the same PR as the change.** If the design moves, `ARCHITECTURE.md` and `LESSONS.md` move with it.
- **Never port dead code.** Record what you dropped and why in `PORT_NOTES.md`.
- **When stuck for more than an hour on a borrow-checker error, stop and re-read 12.1.** The fix is almost always a layout change, not a smarter annotation.

## 16. Case study: getting unstuck at the blaze fight

The blaze fight is where every previous version got stuck, so it's the template for debugging any stuck point. The method: capture automatically, reproduce exactly, compare belief to truth, name the misconception in writing, guard it with a regression test, then prove the fix closed-loop.

### 16.1 What the bot is up against

Put these game facts in `Config` and in the combat context. Check them against the pinned Minecraft version, because mob behavior changes between releases.

- A blaze has 20 HP.
- Its ranged attack cycle: it burns for about 3 seconds, fires three small fireballs over about 0.9 seconds, then waits about 5 seconds before attacking again. If line of sight is broken mid-volley, it suspends the rest of the volley until line of sight returns.
- It can attack from up to 48 blocks. A blaze touching its target does melee damage once per second (6 HP on Normal).
- Shields block its fireballs. Snowballs do 3 damage to blazes. Kills drop 0 or 1 rod without Looting.

Sources: [Minecraft Wiki on Fandom](https://minecraft.fandom.com/wiki/File:NetherAndFortressBlazeAccidentalSkeletonMIME.png), [Craftdex](https://craftdex.net/ru/articles/the-blaze-and-blaze-rods), [wiki mirror](https://wiki.ronlab.site/content/minecraftwiki_en_all_maxi_2025-11/Blaze).

The attack cycle makes this a timing problem. The safe window to close in and strike is the rest period after a volley. The dangerous window is the wind-up, when breaking line of sight or blocking pays off. A bot that doesn't model the cycle will trade hits badly and look "stuck."

### 16.2 Record what the rules saw: `CombatFrame`

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CombatFrame {
    pub tick: Tick,
    pub target: Option<EntityId>,
    pub target_pos: Option<Vec3>,
    pub target_dy: Option<f64>,            // blazes float upward, often above melee reach
    pub dist: Option<f64>,
    pub visible: bool,
    pub target_charging: bool,             // the blaze's "on fire" metadata flag: volley wind-up
    pub incoming: Vec<Projectile>,         // small fireballs: position and velocity
    pub my_health: f32,
    pub on_fire: bool,
    pub attack_ready: bool,                // attack cooldown has recovered
    pub intent: Intent,
    pub rule: Cow<'static, str>,
}
```

Emit one per tick during any fight. Every hypothesis below can be confirmed or ruled out from this stream plus the observations.

### 16.3 Hypotheses to check, with the evidence for each

These are common ways a bot fails against blazes. They are hypotheses, not findings. The recordings decide which ones apply.

| Hypothesis (a misconception) | Evidence in the recording | Typical fix |
| --- | --- | --- |
| **Stale target position.** The bot aims where the blaze was, not where it is. | `LookAt` targets lag behind later `EntityMoved` positions. Attacks register `ExpectationMissed`. | Re-resolve the target every tick, track velocity, aim with lead |
| **Out of reach.** The bot swings at a blaze hovering above melee reach. | `Attack` commands while `target_dy` or `dist` exceeds reach. Repeated `EntityHealthDrops` misses. | A rule for "target above reach": reposition, use a ranged option, or wait for it to descend |
| **Cooldown ignored.** The bot spams attacks, so each hit does little damage. | Several `Attack` commands per second. Small health drops per hit. | Gate melee on `attack_ready` |
| **Standing in the volley.** The bot holds still during wind-up and eats three fireballs. | Health drops in clusters of three. `target_charging` is true while the intent is `Hold` or `Approach`. | During wind-up: break line of sight, strafe or block. Strike during the rest period. |
| **Pathing loop in the fortress.** `Goto` oscillates near the spawner on bridges, fences or lava. | The oscillation trigger fires. `Goto` reports `NoProgress`. | Precompute safe standing spots with cover near the spawner and path to those |
| **Target thrashing.** With several blazes, the target switches every tick. | `RuleFired` and target ids alternate tick to tick. | Hysteresis: keep the current target unless another is clearly better, and commit for a minimum number of ticks |
| **Rods not collected.** Kills happen, but the rod count doesn't rise. | Item entities appear after kills. Inventory doesn't change. | A `PickUp` task after each kill. Give up on items over lava. |
| **Too far from the spawner.** No new blazes spawn. | No new blaze entities near the spawner over time | Stand within the spawner's activation range (16 blocks by default) |
| **Wrong completion predicate.** The bot has enough rods but the phase doesn't advance. | Inventory count vs the `Phase::is_complete` result | Fix the predicate and add a unit test |

### 16.4 The workflow, end to end

The commands and numbers below are illustrative. They show the shape of the work, not results.

```bash
# 1. The watchdog tripped during FightBlaze; the flight recorder wrote an incident folder.
cat incidents/51200-watchdog-FightBlaze/INCIDENT.md

# 2. Reproduce exactly. If this doesn't match, fix determinism before anything else.
bot-replay run recordings/2026-11-02T19-04.jsonl --from-checkpoint 50400 --until 51800 --verify

# 3. Look at it: the 3D view over time, and the combat frames as data.
rerun incidents/51200-watchdog-FightBlaze/window.rrd
bot-replay events recordings/2026-11-02T19-04.jsonl --kind CombatFrame --from 51000 --to 51250 --json \
  | jq -c '{tick, dist, target_dy, visible, target_charging, intent, rule}'

# 4. Write the diagnosis (template below) in docs/diagnoses/.
# 5. Cut the regression window and write the open-loop property test (9.5).
bot-replay cut recordings/2026-11-02T19-04.jsonl --from-checkpoint 50400 --until 51800 \
  -o recordings/regressions/blaze-hold-during-volley.jsonl

# 6. Change the rule table. Confirm the new brain diverges only where intended.
bot-replay diff --old <commit> --new HEAD recordings/regressions/blaze-hold-during-volley.jsonl

# 7. Prove it closed-loop: run the blaze scenario batch before and after the fix.
cargo run -p bot-shell --features scenarios -- scenario blaze-spawner-approach --runs 20
```

### 16.5 Diagnosis template

```markdown
# Diagnosis: <short name>

- Incident: incidents/<dir>; recording <file>; ticks <from>-<to>
- Describe: what the bot did, in observable terms only
- Expected: what a competent player would have done instead
- Beliefs at the decisive tick: (from CombatFrame and the snapshot)
- Truth at that tick: (from later observations, or a server-side check)
- Misconception, one of:
  - a belief was wrong (world model or perception)
  - a rule was wrong given correct beliefs (tactic or planner)
  - the action wasn't carried out as intended (adapter or body)
- Delimit: the single problem this change addresses
- Fix: the rule, model or adapter change
- Evidence: regression test name; scenario success rate before -> after
```

### 16.6 What the blaze rules might grow into

An illustrative shape, not a prescription. Fit the real rules to what the diagnoses find:

```rust
pub const BLAZE_RULES: &[Rule] = &[
    Rule { name: "retreat_low_health",   priority: 100, when: |c| c.health.get() < c.cfg.retreat_hp,        then: |_| Intent::Retreat },
    Rule { name: "break_los_on_windup",  priority: 90,  when: |c| c.target_charging && c.cover.is_some(),   then: |c| Intent::MoveTo(c.cover_pos()) },
    Rule { name: "block_volley",         priority: 85,  when: |c| c.has_shield && !c.incoming.is_empty(),  then: |c| Intent::Block { facing: c.target_pos } },
    Rule { name: "strafe_fireball",      priority: 80,  when: |c| !c.incoming.is_empty(),                  then: |c| Intent::Strafe(c.dodge_dir()) },
    Rule { name: "reposition_under",     priority: 60,  when: |c| c.target_dy > c.cfg.melee_reach,          then: |c| Intent::MoveTo(c.spot_below_target()) },
    Rule { name: "strike_in_rest_window", priority: 50, when: |c| c.in_rest_window() && c.attack_ready && c.dist <= c.cfg.melee_reach, then: |c| Intent::Attack(c.target) },
    Rule { name: "close_in_rest_window", priority: 40,  when: |c| c.in_rest_window(),                       then: |c| Intent::Approach(c.target) },
];
```

Each rule is one line you can read, test in isolation with a fixture (11.1), and see firing in the recording. That is the payoff of the whole architecture: when the bot gets stuck, the question becomes "which rule fired, on what beliefs, and was either wrong?" Every part of that question can be answered from data.

## 17. Checklists and recommended crates

### 17.1 Pull request checklist

Copy this into the PR template.

- [ ] `bot-core` gained no I/O dependency (the `cargo tree` check passes)
- [ ] Clippy passes with the core's `clippy.toml`: no `HashMap`, `HashSet`, `Rc`, `RefCell`, clock reads or unseeded randomness in the core
- [ ] No `_ =>` arms on domain enums, and no new `unwrap`, `expect` or `as` casts in the core
- [ ] New `Observation`, `Command` or `BrainEvent` variants were *added*, not renamed. New fields have `#[serde(default)]`. Old recordings still load.
- [ ] Every new decision emits a `BrainEvent` with a reason
- [ ] Every new world-changing command registers an `Expectation` with a deadline
- [ ] Every new threshold or magic number is a `Config` field
- [ ] Unit tests, plus snapshot or property tests where they apply
- [ ] `bot-replay verify` passes on all golden recordings. All regression recordings pass.
- [ ] `ARCHITECTURE.md`, `LESSONS.md` and `PORT_NOTES.md` are updated if the design moved

### 17.2 Definition of done for a ported Clojure namespace

- [ ] Listed in `INVENTORY.md` with its Rust destination
- [ ] Decision logic is in the core, side effects are in the shell, and nothing mixes them
- [ ] All dispatch keywords became enum variants, matched exhaustively
- [ ] Differential tests against the Clojure version, where the function is pure, with every disagreement explained
- [ ] No `todo!()` left in the module

### 17.3 Recommended crates

Pin exact versions in `Cargo.lock` and check each crate's current docs when you add it. Several of these (Rerun, azalea, rand) rename APIs between releases.

| Crate | Purpose | Allowed in core? |
| --- | --- | --- |
| `serde` (derive) | Everything is serializable data | Yes |
| `rand`, `rand_chacha` (with its serde feature) | One seeded RNG whose state can be checkpointed | Yes |
| `indexmap` | Insertion-ordered maps and sets with deterministic iteration | Yes |
| `slotmap` | Generational keys for internal graphs | Yes |
| `thiserror` | Typed error enums | Yes |
| `libm` | Platform-independent transcendental math, if cross-platform replay matters | Yes |
| `tracing` | Debug breadcrumbs in the core; structured logs in the shell | Yes (debug and trace only) |
| `tokio`, `tokio-util` | Async runtime, channels, `CancellationToken` | **No** |
| `azalea` | Protocol, physics and pathfinding body (option A, needs nightly) | **No** |
| `tracing-subscriber` (features `json`, `env-filter`), `tracing-appender` | Log output and file writing | **No** |
| `anyhow` | Application-level errors in binaries | **No** |
| `serde_json` | JSON Lines recordings (enable the float\_roundtrip feature) | **No** (the shell and replay tool serialize) |
| `postcard`, `zstd` | Compact recordings, later | **No** |
| `axum` | Local read-only debug endpoint | **No** |
| `rerun` | Time-travel 3D visualization | **No** |
| `rayon` | Worker pool for pathfinding, if moved off-thread | **No** |
| `clap` | CLIs for `bot-replay` and scenarios | **No** |
| `insta` + `cargo-insta` | Snapshot tests | Dev only |
| `proptest`, `proptest-derive`, `proptest-state-machine` | Property and state-machine tests | Dev only |
| `criterion` or `divan` | Benchmarks | Dev only |

**Tools:** `cargo-nextest` (fast tests), `bacon` (watch and re-run), `cargo-flamegraph` (profiling), `cargo-deny` (dependency policy, optional).

## 18. Sources and further reading

**Rich Hickey talks** (transcripts in the matthiasn/talk-transcripts repository)

- [Simple Made Easy](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/SimpleMadeEasy.md): complecting, the simplicity toolkit
- [Are We There Yet?](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/AreWeThereYet.md): identity, state, value and the epochal time model
- [The Value of Values](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/ValueOfValuesLong.md): place-oriented programming, facts, the benefits of values
- [The Language of the System](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/LanguageSystem.md): queues, values on the wire, simple services, partial failure
- [Deconstructing the Database](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/DeconstructingTheDatabase.md): the log as source of truth, derived indexes, perception vs process
- [Effective Programs](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/EffectivePrograms.md): situated programs, ranked problems of programming, runtime tangibility
- [Spec-ulation](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/Spec_ulation.md): growth vs breakage, accretion
- [Hammock Driven Development](https://github.com/matthiasn/talk-transcripts/blob/master/Hickey_Rich/HammockDrivenDev.md): stating and understanding problems before solving them
- [Maybe Not](https://neonvagabond.xyz/mirrors/maybe-not.html): optionality depends on context; schema vs selection
- [Design in Practice (slides)](https://download.clojure.org/presentations/DesignInPractice.pdf): describe, diagnose, delimit, direction, design, develop
- [Transducers](https://clojure.org/reference/transducers): context-independent transformations

**Rust design**

- [Catherine West, RustConf 2018 closing keynote notes](https://kyren.github.io/2018/09/14/rustconf-talk.html): why object graphs fight Rust; ECS, flat data and generational indices
- [Firezone: sans-IO, the secret to effective Rust for network services](https://firezone.dev/blog/sans-io): pure state machines, time as input, event loops
- [Alice Ryhl: Actors with Tokio](https://ryhl.io/blog/actors-with-tokio/): tasks with mailboxes instead of shared locks
- [Aliasing XOR mutability discussion](https://blog.kodewerx.org/2025/), with pointers to Niko Matsakis and Manish Goregaokar on shared mutability
- [Comprehensive Rust: Parse, Don't Validate](https://google.github.io/comprehensive-rust/idiomatic/leveraging-the-type-system/newtype-pattern/parse-don-t-validate.html): newtypes enforcing invariants
- [Cliffle: The Typestate Pattern in Rust](https://cliffle.com/tags/type-systems)

**Libraries and tools**

- [azalea on docs.rs](https://docs.rs/crate/azalea/latest): Rust Minecraft bot framework (0.16.0+mc26.1 at time of writing)
- [Rerun](https://github.com/rerun-io/rerun) and its [Rust `RecordingStream` API](https://docs.rs/rerun/latest/rerun/struct.RecordingStream.html) and [archetypes](https://docs.rs/rerun/latest/rerun/archetypes/index.html) (0.43.1 at time of writing)
- [Proptest book: state machine testing](https://proptest-rs.github.io/proptest/proptest/state-machine.html)
- [insta snapshot testing](https://docs.rs/insta/1.47.2/src/insta/lib.rs.html)

**Determinism and simulation testing**

- [NautilusTrader: deterministic simulation testing](https://nautilustrader.io/docs/latest/concepts/dst/): iteration-order rules
- [Murk: determinism](https://tachyon-beep.github.io/murk/determinism.html): banning `HashMap` with clippy's `disallowed-types`
- [Moonpool: reproducing a failing seed](https://pierrez.github.io/moonpool/part3-building/21-reproducing.html)
- [Determinism by order](https://root-11.github.io/intro-book/trunk/16_determinism_by_order.html): input logs plus snapshots make replay structural

**Minecraft blaze mechanics**

- [Minecraft Wiki on Fandom (blaze excerpt)](https://minecraft.fandom.com/wiki/File:NetherAndFortressBlazeAccidentalSkeletonMIME.png)
- [Craftdex: the blaze and blaze rods](https://craftdex.net/ru/articles/the-blaze-and-blaze-rods)
- [Minecraft Wiki mirror: Blaze](https://wiki.ronlab.site/content/minecraftwiki_en_all_maxi_2025-11/Blaze)
