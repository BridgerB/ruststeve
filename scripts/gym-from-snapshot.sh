#!/usr/bin/env bash
# gym-from-snapshot.sh <snapshot.env> [step_id] [secs] [extra "give" items...]
#
# Replays a snapshot.sh capture: kits a gym bot with the recorded inventory, writes its frame
# anchor (translated into the slot), tp's it to the recorded offset inside the slot, and runs
# ONE step in STEVE_TEST mode against Server B until it passes or `secs` elapse. Log:
# gym-snap-<n>.log. Re-runnable: re-clone the slot first with `--reclone` to reset the terrain.
#   extra items: e.g. "lava_bucket 2" "cooked_beef 8"  (lava buckets aren't in race.db ticks)
set -u
DIR=$(cd "$(dirname "$0")/.." && pwd)
RCON=$DIR/target/release/rcon
BIN=$DIR/target/release/ruststeve
SNAP=${1:?usage: gym-from-snapshot.sh <snapshot.env> [step_id] [secs] [\"item n\" ...]}
# shellcheck disable=SC1090
source "$SNAP"
STEP_ID=${2:-$STEP}; SECS=${3:-900}; shift 3 2>/dev/null || shift $#
NAME=${SNAP_BOT:-rust-snap-$(printf '%03d' "$SLOT")}
read -r DXX DYY DZZ <<< "$BOT_DST"
read -r SX0 SY0 SZ0 SX1 SY1 SZ1 <<< "$SRC"
read -r DX0 DY0 DZ0 <<< "$DST"
if [ "${RECLONE:-0}" = "1" ]; then
  echo "[gym-snap] re-cloning slot $SLOT from source"
  "$RCON" "clone $SX0 $SY0 $SZ0 $SX1 $SY1 $SZ1 $DX0 $DY0 $DZ0 replace force" >/dev/null
fi
pick_name() { case "$1" in 1) echo wooden_pickaxe;; 2) echo stone_pickaxe;; 3) echo iron_pickaxe;; 4) echo diamond_pickaxe;; *) echo "";; esac; }
sword_name() { case "$1" in 1) echo wooden_sword;; 2) echo stone_sword;; 3) echo iron_sword;; 4) echo diamond_sword;; *) echo "";; esac; }
give=()
g() { [ "${2:-0}" -gt 0 ] 2>/dev/null && give+=("give $NAME minecraft:$1 $2"); }
p=$(pick_name "$PICK"); [ -n "$p" ] && give+=("give $NAME minecraft:$p")
s=$(sword_name "$SWORD"); [ -n "$s" ] && give+=("give $NAME minecraft:$s")
g oak_log "$LOGS"; g oak_planks "$PLANKS"; g stick "$STICKS"; g cobblestone "$COBBLE"; g dirt "$DIRT"
g coal "$COAL"; g raw_iron "$IRON_ORE"; g iron_ingot "$INGOTS"; g cooked_beef "$FOOD"; g crafting_table "$TABLES"
g bucket "$BUCKETS"; g water_bucket "$WBUCKETS"; g flint "$FLINT"; g flint_and_steel "$FNS"
for extra in "$@"; do give+=("give $NAME minecraft:$extra"); done
[ -n "$ANCHOR_DST" ] && echo "$ANCHOR_DST" > "$DIR/.frame-$NAME.txt" || rm -f "$DIR/.frame-$NAME.txt"
rm -f "$DIR/.memory-$NAME.db"*
LOG=$DIR/gym-snap-$SLOT.log; : > "$LOG"
pkill -9 -f "MC_USERNAME=$NAME" 2>/dev/null
"$RCON" "op $NAME" "forceload add $((DX0-1)) $((DZ0-1)) $((DX0+2*10+1)) $((DZ0+2*10+1))" >/dev/null
echo "[gym-snap] launching $NAME → step=$STEP_ID secs=$SECS at ($DXX,$DYY,$DZZ) anchor=[$ANCHOR_DST]"
STEVE_TEST="$STEP_ID" STEVE_TEST_SECS="$SECS" CRAFT_DEBUG=1 RACE_HOLD=20 \
  MC_HOST=144.24.32.76 MC_PORT=25566 MC_USERNAME="$NAME" STEVE_DATA="$DIR/data" \
  RCON_HOST=localhost RCON_PORT=25576 "$BIN" >> "$LOG" 2>&1 &
PID=$!
for t in $(seq 1 30); do grep -q 'holding' "$LOG" 2>/dev/null && break; sleep 2; done
"$RCON" "clear $NAME" "${give[@]}" "tp $NAME $DXX $DYY $DZZ" "spawnpoint $NAME $DXX $DYY $DZZ" >/dev/null
sleep 2
"$RCON" "tp $NAME $DXX $DYY $DZZ" >/dev/null
echo "[gym-snap] running (pid $PID, log $LOG) — result line: grep 'TEST RESULT' $LOG"
