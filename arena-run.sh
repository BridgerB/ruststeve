#!/usr/bin/env bash
# OVERWORLD blaze-arena harness (ARENA_TEST): validates the kill_blaze combat where world chunks
# load reliably (the cross-dimension tp into the nether streams chunks too slowly — separate SDK
# bug). Builds a sealed nether-brick arena + blaze spawner in the overworld, drops a kitted,
# invulnerable bot in, and keeps it alive/kitted/fighting across connection drops.
set -u
HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
M='sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon'
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
CX=500; CY=100; CZ=500; SX=$((CX+1)); SZ=$((CZ+1))   # arena center + bot stand
cd "$DIR" || exit 1
rc() { $SSH "$M $*" 2>&1 | tr -d '\033' | sed 's/\[0m//g'; }

pkill -9 -f 'target/release/ruststeve' 2>/dev/null
for p in $(pgrep -f 'arena-run.sh'); do [ "$p" != "$$" ] && kill -9 "$p" 2>/dev/null; done
sleep 2

# Build the sealed arena once (netherrack shell, air interior, nether-brick floor) + spawner.
rc "\"difficulty normal\" \"gamerule keep_inventory true\" \"op rust-race-001\"" >/dev/null
rc "\"forceload add 492 492 508 508\" \
    \"fill 495 96 495 505 104 505 minecraft:netherrack\" \
    \"fill 497 100 497 503 102 503 minecraft:air\" \
    \"fill 497 99 497 503 99 503 minecraft:nether_bricks\"" >/dev/null
$SSH "$M \"setblock $CX $CY $CZ minecraft:spawner{SpawnData:{entity:{id:\\\"minecraft:blaze\\\"}},SpawnCount:2,MaxNearbyEntities:6,RequiredPlayerRange:16,SpawnRange:2,MinSpawnDelay:40,MaxSpawnDelay:80}\"" >/dev/null 2>&1

launch() {
  MC_HOST=localhost MC_USERNAME=rust-race-001 STEVE_DATA=data RACE_GOAL=blaze ARENA_TEST=1 CRAFT_DEBUG=1 \
    RUST_VIEW=1 RUST_VIEW_ASSETS="$DIR/viewer/static/assets.json" \
    nohup "$BIN" >>/tmp/nether-0.log 2>&1 &
}
setup() {  # kit + place in the arena + invulnerable (survives fire/void so a fight can finish)
  rc "\"tp rust-race-001 $SX $CY $SZ\" \"spawnpoint rust-race-001 $SX $CY $SZ\" \
      \"give rust-race-001 minecraft:iron_sword\" \"give rust-race-001 minecraft:iron_chestplate\" \
      \"give rust-race-001 minecraft:cooked_porkchop 16\" \
      \"effect give rust-race-001 minecraft:fire_resistance infinite 255 true\" \
      \"effect give rust-race-001 minecraft:resistance infinite 255 true\" \
      \"data merge entity rust-race-001 {Invulnerable:1b}\"" >/dev/null
}

: > /tmp/nether-0.log
while true; do
  if ! ps ewwx | grep -q "[M]C_USERNAME=rust-race-001"; then
    echo "$(date +%H:%M:%S) (re)launch bot" >> /tmp/arena-watchdog.log
    launch
    for _ in $(seq 1 30); do rc "list" | grep -q rust-race-001 && break; sleep 2; done
    sleep 8; setup
  fi
  # top up: keep it invulnerable/kitted + ensure blazes present (summon a couple each cycle)
  rc "\"data merge entity rust-race-001 {Invulnerable:1b}\" \
      \"execute in minecraft:overworld run summon minecraft:blaze $CX 101 $CZ\"" >/dev/null 2>&1
  # win check
  if grep -q "RACE GOAL REACHED: blaze" /tmp/nether-0.log; then
    echo "$(date +%H:%M:%S) *** BLAZE ROD — GOAL REACHED ***" >> /tmp/arena-watchdog.log
  fi
  sleep 15
done
