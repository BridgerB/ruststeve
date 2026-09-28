#!/usr/bin/env bash
# Endurance rig on SERVER B: N kitted bots run GYM=to_nether (full natural cast →
# ENTER the nether on random surface terrain; PASS = in_nether). Many bots × trials
# across varied terrain = the proven path to getting a bot through (how race-011 did
# it). Game port 25566 direct; RCON over the tunnel localhost:25576. No SNIFF (long
# trials would bloat the db); CAST debug on for the cast logs. Never touches Server A.
set -u
N=${N:-6}
TRIALS=${TRIALS:-4}
SLUG=${SLUG:-to_nether}
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
cd "$DIR" || exit 1

pkill -9 -f 'target/release/ruststeve' 2>/dev/null; sleep 1
for i in $(seq 1 "$N"); do
  name=$(printf 'rust-gym-%03d' "$i")
  rm -f ".memory-$name.db"* "gym-$name.log" 2>/dev/null
  MC_HOST=144.24.32.76 MC_PORT=25566 MC_USERNAME="$name" STEVE_DATA="$DIR/data" \
    RCON_HOST=localhost RCON_PORT=25576 \
    GYM="$SLUG" GYM_TRIALS="$TRIALS" CRAFT_DEBUG=1 \
    "$BIN" > "gym-$name.log" 2>&1 &
  echo "launched $name pid $!"
  sleep 4
done
echo "all $N launched (slug=$SLUG trials=$TRIALS each) on Server B"
