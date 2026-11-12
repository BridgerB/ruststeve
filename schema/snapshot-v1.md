# The live snapshot stream, v1

The contract between a running ruststeve bot and steve's relay for the **local** live view (issue #12; brain/ruststeve/design/snapshot-stream.md). The bot connects out to `$STEVE_RELAY/ingest/<bot-id>` and only sends; it never reads. `STEVE_RELAY` unset means no stream, and the bot behaves exactly the same. Runners never set it: the live view is local only (`STEVE_RELAY=ws://localhost:8787`).

`<bot-id>` is `<run>-<bot>`: the race run id (or `local-<ms>` for a single bot) and the bot's name, the same pair the insight store keys a run by (`run:⟨<run>/<bot>⟩`).

Grows by accretion only: add kinds and fields, never rename or repurpose one. A change bumps `v` and is posted on the issue that makes it. steve reads `v` and ignores what it does not know.

## Frames

One binary WebSocket message per frame: a MessagePack map with named fields (the recordings' style), not compressed.

Every frame carries the envelope:

| field | type | meaning |
|---|---|---|
| `v` | u16 | schema version, 1 |
| `kind` | string | one of the kinds below |
| `bot` | string | the bot's name |
| `run` | string | the race run id, or `local-<ms>` |
| `tick` | u64 | server ticks the bot has counted (20 per `set_time`) |
| `seq` | u64 | per-connection counter from 0 |

## Kinds

| kind | when | fields |
|---|---|---|
| `hello` | once per connection, first | `commit` (full sha), `mc_version`, `seed` (the run's shell seed), `lane` (u32), `start` (start name), `stage` (stage reached or ""), `source` (`real`), `server` (host:port label) |
| `self` | at most 5 a second; also right after `hello` | `pos` [x, y, z], `yaw`, `pitch`, `dim`, `on_ground`, `health` (f32, -1 unknown), `food` (i32, -1 unknown), `air` (i32, -1: not read yet), `held` (item name or ""), `inv` {item name: count}, `stage` (stage reached or ""), `tasks` [{`name`, `why`}] top first, `reason` (why the top task) |
| `chunk` | each column received; all loaded columns again after a reconnect | `x`, `z` (column coordinates), `data` (bin: the raw `level_chunk_with_light` packet body as received, decode with typecraft) |
| `unload` | a column forgotten | `x`, `z` |
| `block` | a block changed | `x`, `y`, `z`, `state` (block state id) |
| `entities` | at most 10 a second, coalesced | `list`: [{`id`, `kind` (entity name), `pos` [x, y, z], `hostile` (bool: the game's monster category)}] |
| `gone` | an entity removed | `id` |
| `event` | a decision the bot made (BrainEvents; none before the planner, issue 09) | `name`, `reason` |
| `end` | once, when the run ends | `ending` (death, cap, kicked, lost, error, cut), `killer` (the server's death text first, else ""), `stage_reached` (or ""), `case` (where the case file is: a path, or "" for a single bot) |

## The sink's rules (the bot side)

- An observer: it reads the world after each step and never feeds back; a recording verifies the same with the sink on, off or failing.
- Never blocks the tick: a bounded channel to a sender thread; when it is full, frames are dropped and counted.
- Reconnects with backoff, 1 s doubling to 30 s; after each connect sends `hello`, the latest `self` and every loaded `chunk`.
- No secret in any frame.
- Counts frames sent, dropped (channel full) and failed (no connection) into the run's case (`stream_*`).
