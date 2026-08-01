#!/usr/bin/env bash
# LAVA-FIND fast test — the piece the normal isolation test can't cover.
#
#   ./lava-find-test.sh [SECS]
#
# The `build_nether_portal` isolation test HANDS the bot a lava lake, so it only
# validates the cast, not FINDING lava — which is the real live blocker. This test
# drops an iron-pick-ready bot into NATURAL terrain (no arena, NO lava placed) and
# makes it locate real lava on its own, then cast + light. Minute-scale feedback on
# prepare_cast_site / descend_to_y instead of waiting ~90 min for a race.
#
# COEXISTENCE-SAFE: distinct name (rust-lava-01), scoped forceload (NEVER remove all),
# location well clear of the race lanes (z350-710) and the TS agent. Never wipes world.
set -u
SECS="${1:-420}"
HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
MCRCON="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
DATA=$DIR/data
NAME=rust-lava-01
# Natural forest band (same x as the race, but z=290 — clear of race lanes z350-710,
# and known land: the race bots spawn fine in this band). Falls to the ~y74 surface.
X=680; Z=290; Y=80
cd "$DIR" || exit 1
rcon() { $SSH "$MCRCON $*" 2>&1; }

echo "[lava-test] $NAME at ($X,~,$Z), budget ${SECS}s — must find real lava + cast"
pkill -9 -f "MC_USERNAME=$NAME" 2>/dev/null
# scoped forceload ONLY around the test spot (never 'remove all')
rcon "'forceload add $((X-30)) $((Z-30)) $((X+30)) $((Z+30))'" >/dev/null
sleep 4

: > "$DIR/lava-find.log"
STEVE_TEST=build_nether_portal STEVE_TEST_SECS="$SECS" CRAFT_DEBUG=1 RACE_HOLD=30 \
  MC_HOST=$HOST MC_PORT=25565 MC_USERNAME="$NAME" STEVE_DATA="$DATA" \
  "$BIN" >> "$DIR/lava-find.log" 2>&1 &
PID=$!

# wait for spawn+chunks
for t in $(seq 1 25); do grep -q 'holding' "$DIR/lava-find.log" 2>/dev/null && break; sleep 2; done

echo "[lava-test] positioning + iron-pick portal prereqs (NO lava given)"
rcon "'op $NAME' 'tp $NAME $X $Y $Z' 'spawnpoint $NAME $X $Y $Z' 'clear $NAME' \
  'give $NAME minecraft:iron_pickaxe' 'give $NAME minecraft:water_bucket' \
  'give $NAME minecraft:bucket' 'give $NAME minecraft:flint_and_steel' \
  'give $NAME minecraft:cobblestone 128'" >/dev/null

echo "[lava-test] running…"
END=$((SECS+40))
for s in $(seq 0 10 $END); do
  grep -q 'TEST RESULT:' "$DIR/lava-find.log" 2>/dev/null && break
  sleep 10
done

echo ""
echo "===== LAVA-FIND RESULT ====="
grep 'TEST RESULT:' "$DIR/lava-find.log" 2>/dev/null | tail -1 || echo "(no result — still running/hung)"
echo "-- lava behavior --"
echo "prepare attempts: $(grep -c 'prepare: at' lava-find.log 2>/dev/null)"
echo "remembered-lava hits: $(grep -c 'remembered lava' lava-find.log 2>/dev/null)"
echo "pool found: $(grep -c 'prepare: pool' lava-find.log 2>/dev/null)"
echo "no-lava fails: $(grep -c 'no lava pool' lava-find.log 2>/dev/null)"
grep -E 'prepare: at|remembered lava|prepare: pool|bowl=|cast .* a[0-9]|scooped' lava-find.log 2>/dev/null | tail -8
pkill -9 -f "MC_USERNAME=$NAME" 2>/dev/null
