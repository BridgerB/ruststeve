#!/usr/bin/env bash
# Fast scoop-iteration rig on SERVER B: N kitted bots run GYM=reach_lava (descend
# to lava, scoop ONE lava bucket = pass). Game port 25566 (direct, IP-allowlisted);
# RCON over the SSH tunnel localhost:25576. Never touches Server A. CAST debug on;
# sniff the scoop packet exchange to data/sniff-<name>.db.
set -u
N=${N:-3}
TRIALS=${TRIALS:-2}
SLUG=${SLUG:-reach_lava}
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
cd "$DIR" || exit 1

pkill -9 -f 'target/release/ruststeve' 2>/dev/null; sleep 1
for i in $(seq 1 "$N"); do
  name=$(printf 'rust-gym-%03d' "$i")
  rm -f ".memory-$name.db"* "gym-$name.log" "data/sniff-$name.db"* 2>/dev/null
  MC_HOST=144.24.32.76 MC_PORT=25566 MC_USERNAME="$name" STEVE_DATA="$DIR/data" \
    RCON_HOST=localhost RCON_PORT=25576 \
    GYM="$SLUG" GYM_TRIALS="$TRIALS" CRAFT_DEBUG=1 \
    SNIFF=use_item,acknowledge,block_update,carried \
    "$BIN" > "gym-$name.log" 2>&1 &
  echo "launched $name pid $!"
  sleep 4   # stagger so spreadplayers/give/tp don't collide on RCON
done
echo "all $N launched (slug=$SLUG trials=$TRIALS each) on Server B"
