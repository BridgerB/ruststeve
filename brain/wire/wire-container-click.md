---
title: Container clicks
description: What a 775 container-click carries, why every bot ended up claiming an empty prediction, and the one-click-behind trap of reading only the first reply.
type: reference
tags: [wire, windows, crafting, clicks]
aliases: [container-click, state id, changed slots, predict nothing, set-cursor-item, container-set-content]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f
sourceRefs:
  - "clojurecraft:src/clojurecraft/packet.clj#;; 775 clicks carry the client's prediction as hashed slots; we always predict nothing"
  - "clojurecraft:src/clojurecraft/packet.clj#[[:window-id :varint] [:state-id :varint] [:slot :i16] [:button :i8] [:mode :varint]"
  - "ruststeve-v1:src/bot/inventory.rs#// Send EMPTY changedSlots so the server ALWAYS sees a prediction mismatch and"
  - "ruststeve-v1:src/bot/inventory.rs#/// Then keep reading until the window has been quiet for 100 ms: one click can draw several packets"
  - "clojurecraft:src/clojurecraft/window.clj#way to learn the cursor after a lost click, since click forgets it as it sends."
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-slots]]"
  - "[[cc-window-clicks]]"
  - "[[hist-v1-load-bearing-fixes]]"
---

# Container clicks

A click is `container-click`: window id, **state id**, slot (i16), button (i8), mode (varint), then the client's *prediction*: a list of changed slots as hashed slots and the predicted cursor.

## Key files
- clojurecraft `src/clojurecraft/packet.clj` (the spec) and `window.clj` (`click`).
- ruststeve v1 `src/bot/inventory.rs` (`click_window`, `wait_for_inventory_ack`).

## How it works
- The server stamps every window update (`container-set-content`, `container-set-slot`) with a state id; the client echoes the last one it saw in each click.
- Where the prediction disagrees with the server's result, the server answers with authoritative slots. So claiming **no changed slots** makes every click draw an authoritative reply. ruststeve v1 found this under load (its optimistic prediction desynced into missing-ingredient and result-never-appeared failures) and switched to empty `changedSlots`; clojurecraft predicts nothing from the start (`:changed []`, `:cursor nil`) and changes its window model only from server packets.
- One click can draw several packets (a set-slot per changed slot, or a full resync). Reading only the first leaves the rest as the *next* click's reply, and the client runs one click behind (v1's decision-7 packet capture: planks piled into grid slot 3 and the result was an oak button). v1's fix: keep reading until the window has been quiet for 100 ms.

## Gotchas
- clojurecraft sends `:cursor nil` while v1 still sent its predicted cursor. Because the claim is empty, clojurecraft's `click` forgets its cursor as it sends; after a lost click, `resync-click` sends a state id the server's window never has, which makes the server answer with the whole window and the real cursor.
- A click into a window the server already closed is silently ignored ([[hist-v1-load-bearing-fixes]]).

## See also
- [[cc-window-clicks]] - clojurecraft's click rules and the 3 s stale-window deadline.
- [[wire-slots]] - the hashed-slot format.
