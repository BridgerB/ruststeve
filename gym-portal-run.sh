#!/usr/bin/env bash
# Launch N portal-gym bots over the SSH tunnel (localhost). Each self-RCONs spreadplayers/give.
set -u
N=${N:-8}
TRIALS=${TRIALS:-2}
DATA=/Users/bridger/Developer/mc/upstream/ruststeve/data
BIN=/Users/bridger/Developer/mc/upstream/ruststeve/target/release/ruststeve
cd /Users/bridger/Developer/mc/upstream/ruststeve || exit 1
pkill -9 -f "target/release/ruststeve" 2>/dev/null; sleep 1
for i in $(seq 1 "$N"); do
  name=$(printf "rust-gym-%03d" "$i")
  rm -f ".memory-$name.db"* "gym-$name.log" 2>/dev/null
  MC_HOST=localhost MC_PORT=25565 MC_USERNAME="$name" STEVE_DATA="$DATA" \
    GYM=portal GYM_TRIALS="$TRIALS" CRAFT_DEBUG=1 \
    "$BIN" > "gym-$name.log" 2>&1 &
  echo "launched $name pid $!"
  sleep 3   # stagger so spreadplayers/give don't collide on RCON
done
echo "all $N launched (TRIALS=$TRIALS each)"
