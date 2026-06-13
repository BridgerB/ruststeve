#!/usr/bin/env bash
# Fast isolated test of cast_obsidian_at: flat arena, bot given buckets+cobble,
# casts ONE block 2 north of it. ~1 min cycle.
set -u
HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
MCRCON="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
DATA=$DIR/../rustcraft/data
NAME=ptest
X=305; Y=70; Z=305

cd "$DIR" || exit 1
pkill -9 -f "MC_USERNAME=$NAME" 2>/dev/null
$SSH "$MCRCON 'kick $NAME'" >/dev/null 2>&1; sleep 2
rm -f "$DIR/.memory-$NAME.db" "$DIR/.memory-$NAME.db-wal" "$DIR/.memory-$NAME.db-shm" "$DIR/ptest.log" 2>/dev/null

echo "[cast-one] forceload + flat arena (clear any leftover blocks)"
$SSH "$MCRCON 'forceload remove all' 'forceload add 280 280 340 340'" >/dev/null 2>&1; sleep 5
$SSH "$MCRCON \
  'fill 290 $((Y-1)) 290 330 $((Y-1)) 330 minecraft:stone' \
  'fill 290 $Y 290 330 $((Y+8)) 330 minecraft:air' \
  'gamerule doFireTick false' " >/dev/null 2>&1

echo "[cast-one] launch (CAST_ONE)"
CAST_ONE=1 CAST_SNIFF=1 MC_HOST=$HOST MC_PORT=25565 MC_USERNAME=$NAME STEVE_DATA="$DATA" \
  RACE_HOLD=30 RACE_GOAL=nether CRAFT_DEBUG=1 \
  "$BIN" >> "$DIR/ptest.log" 2>&1 &
echo "[cast-one] pid $!"
for t in $(seq 1 20); do grep -q 'holding' "$DIR/ptest.log" 2>/dev/null && break; sleep 2; done
sleep 2

echo "[cast-one] drop bot + give buckets/cobble (1 lava, 1 water, 1 empty)"
$SSH "$MCRCON \
  'op $NAME' \
  'tp $NAME $X $Y $Z' 'spawnpoint $NAME $X $Y $Z' 'clear $NAME' \
  'give $NAME minecraft:iron_pickaxe' \
  'give $NAME minecraft:lava_bucket' \
  'give $NAME minecraft:water_bucket' \
  'give $NAME minecraft:bucket' \
  'give $NAME minecraft:flint_and_steel' 'give $NAME minecraft:cobblestone 64' \
  " >/dev/null 2>&1
echo "[cast-one] casting block at $X $Y $((Z-2)) — watch ptest.log"
