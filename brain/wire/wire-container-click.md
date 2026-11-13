---
title: Container clicks
description: What a 775 container-click carries, why every bot ended up claiming an empty prediction, and the one-click-behind trap of reading only the first reply.
type: reference
tags: [wire, windows, crafting, clicks]
aliases: [container-click, state id, changed slots, predict nothing, set-cursor-item, container-set-content]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f ruststeve@34b2252
sourceRefs:
  - "ruststeve:crates/protocol/src/packets.rs#w.varint(0); // no changed slots: nothing predicted"
  - "ruststeve:crates/world/src/lib.rs#fn window_moved(&mut self, window: i32, state_id: i32) {"
  - "ruststeve:crates/protocol/tests/encode.rs#fn a_click_predicts_nothing() {"
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
- ruststeve `crates/protocol/src/packets.rs` (`C2s::ContainerClick`: `varint 0` changed slots and an empty hashed cursor, checked byte for byte by `a_click_predicts_nothing`) and `crates/world/src/lib.rs` (`Act::Click` stamps the window's last state id and holds a `PendingClick`; `window_moved` clears it when a window-0 content or slot packet brings a new state id).

## How it works
- The server stamps every window update (`container-set-content`, `container-set-slot`) with a state id; the client echoes the last one it saw in each click.
- Where the prediction disagrees with the server's result, the server answers with authoritative slots. So claiming **no changed slots** makes every click draw an authoritative reply. ruststeve v1 found this under load (its optimistic prediction desynced into missing-ingredient and result-never-appeared failures) and switched to empty `changedSlots`; clojurecraft predicts nothing from the start (`:changed []`, `:cursor nil`) and changes its window model only from server packets.
- One click can draw several packets (a set-slot per changed slot, or a full resync). Reading only the first leaves the rest as the *next* click's reply, and the client runs one click behind (v1's decision-7 packet capture: planks piled into grid slot 3 and the result was an oak button). v1's fix: keep reading until the window has been quiet for 100 ms.

## Gotchas
- clojurecraft sends `:cursor nil` while v1 still sent its predicted cursor. Because the claim is empty, clojurecraft's `click` forgets its cursor as it sends; after a lost click, `resync-click` sends a state id the server's window never has, which makes the server answer with the whole window and the real cursor.
- A click into a window the server already closed is silently ignored ([[hist-v1-load-bearing-fixes]]).
- **The cursor is forgotten as the click goes** (ruststeve, issue 10b): the server takes the click's claimed cursor (empty) as the client's and sends `set_cursor_item` only when the real cursor differs, so an emptied cursor is never announced. The world sets its cursor to empty on every click; a non-empty one comes back in the reply. Found by the parity table: crafts ended with a phantom ingredient in the bot's cursor.
- **Other windows** (issue 11a): a click goes to the open window with that window's state id; the cursor is the open menu's, so `set_cursor_item` applies whichever window is open ([[table-window]]).
- **The resync** (ruststeve's `Act::Resync`, issue 10c): a click on slot -1 (which does nothing) with state id -1 (which the server never has) draws the whole window, for after a missed click ([[crafting-window]]).
- **A click that changes nothing draws no reply** (ruststeve, live on a local 26.1.2 server, 2026-11-12): a pick-up on an empty slot with an empty cursor left the state id at 1 and no window packet came. So a pending click is cleared only by the state id moving, never assumed answered; the craft task sends no no-op clicks and treats 3 s without a move as a missed expectation ([[cc-window-clicks]]).

## See also
- [[cc-window-clicks]] - clojurecraft's click rules and the 3 s stale-window deadline.
- [[wire-slots]] - the hashed-slot format.
