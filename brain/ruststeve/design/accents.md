---
title: Accents to listen for
description: The TypeScript, OOP and Clojure habits that look like Rust but are not, each with what to do instead; the review list for every module.
type: reference
tags: [anti-patterns, review, style]
aliases: [anti-patterns, TypeScript accent, Clojure accent, code smells, review checklist]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Rust v1 failed because it spoke Rust with a TypeScript accent."
  - "ruststeve:FUNCTIONAL.md#The Clojure ideas transfer. Many Clojure mechanisms are unnecessary in Rust because ownership already gives you what they were for."
  - "ruststeve:FUNCTIONAL.md#A module over about 800 lines, or a function over about 80 lines, that mixes perception, decision and action"
  - "ruststeve:FUNCTIONAL.md#Copy this into the PR template."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[hist-why-v1-failed]]"
  - "[[cc-to-rust-map]]"
---

# Accents to listen for

The first Rust port failed because it spoke Rust with a TypeScript accent. This port risks a Clojure one. Review every module against these.

## Key files
- `FUNCTIONAL.md`, section 13 (accents) and 17.1 (PR checklist).

## How it works

**TypeScript accent** (what sank v1): `Arc<Mutex<Bot>>` everywhere → one owner; callbacks that mutate state → callbacks only push into a channel; `async` decision logic → sans-IO core; `serde_json::Value` in the core → typed enums; strings for kinds → `EntityKind::Blaze`; sleeps for delays → tick deadlines; panics or `?` for game outcomes → `Failed(Failure)`.

**OOP accent**: `trait Entity { fn update(&mut self, &mut World) }` → data plus functions; getters and setters → public fields; one-method `Manager`/`Service` → a function; visitor → `match`; observers → events in `Output`; parent back-pointers → ids; lifetimes on domain structs → owned data or ids; a builder per struct → struct literals (builders only for test fixtures).

**Clojure accent**: `HashMap<String, Value>` → structs and enums; `im` collections everywhere → `Vec`, `BTreeMap`, `Arc::make_mut`; `.clone()` "to stay immutable" → mutate in place inside `step`; `fn f(self) -> Self` chains → `&mut self`; macros for `->` → plain code; `HashMap<&str, fn>` dispatch → `match`; `Option<Option<T>>` → enum variants that say what absence means; laziness everywhere → collect when storing; hunting for a REPL → replay tool, `insta`, the snapshot endpoint.

**General smells**: `.clone()` only to silence the borrow checker (restructure instead: split the borrow, perceive then act, use an id, move out and back); `unwrap`, `expect` or `as` in the core; `_ =>` on a domain enum; a trait with one implementation and no test double; `#[allow]` without a reason; a module over ~800 lines or a function over ~80 that mixes perception, decision and action.

**PR checklist**: no I/O dependency in `bot-core`; clippy clean with the core's `clippy.toml`; no wildcard arms or new unwraps; variants added not renamed, new fields `#[serde(default)]`, old recordings load; every decision emits an event with a reason; every world-changing command registers an expectation; every threshold is a `Config` field; tests; `verify` passes on golden and regression recordings; docs moved with the design.

## See also
- [[hist-why-v1-failed]] - the accent in practice.
- [[cc-to-rust-map]] - the idiom table this checks against.
