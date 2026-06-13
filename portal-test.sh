#!/usr/bin/env bash
# Controlled single-bot test of obsidian-casting nether portal.
# Builds a force-loaded flat arena with a contained lava pool, drops the bot in
# with the portal materials, and lets it run build_nether_portal.
set -u
HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
MCRCON="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
DATA=$DIR/../rustcraft/data
NAME=ptest
X=305; Y=70; Z=305   # bot stands here; floor at Y-1

cd "$DIR" || exit 1
pkill -9 -f "MC_USERNAME=$NAME" 2>/dev/null; sleep 1
rm -f "$DIR/.memory-$NAME.db" "$DIR/ptest.log" 2>/dev/null

echo "[ptest] forceload arena + build platform with contained lava pool"
$SSH "$MCRCON 'forceload remove all' 'forceload add 280 280 340 340'" >/dev/null 2>&1
sleep 6
# Sub-floor + floor stone, air above, then a lava pit SET INTO the floor (air
# above it) so its surface is exposed — find_fluid needs to see the lava.
$SSH "$MCRCON \
  'fill 290 $((Y-2)) 290 330 $((Y-2)) 330 minecraft:stone' \
  'fill 290 $((Y-1)) 290 330 $((Y-1)) 330 minecraft:stone' \
  'fill 290 $Y 290 330 $((Y+7)) 330 minecraft:air' \
  'fill $((X+5)) $((Y-1)) $((Z-2)) $((X+8)) $((Y-1)) $((Z+2)) minecraft:lava' \
  " 2>&1 | tail -2
$SSH "$MCRCON 'gamerule doFireTick false'" >/dev/null 2>&1

echo "[ptest] launch + hold"
MC_HOST=$HOST MC_PORT=25565 MC_USERNAME=$NAME STEVE_DATA="$DATA" \
  RACE_HOLD=45 RACE_GOAL=nether CRAFT_DEBUG=1 \
  "$BIN" >> "$DIR/ptest.log" 2>&1 &
echo "[ptest] pid $! — waiting for hold"
for t in $(seq 1 25); do grep -q 'holding' "$DIR/ptest.log" 2>/dev/null && break; sleep 2; done
sleep 2

echo "[ptest] drop bot in arena + give materials"
$SSH "$MCRCON \
  'op $NAME' \
  'tp $NAME $X $Y $Z' \
  'spawnpoint $NAME $X $Y $Z' \
  'clear $NAME' \
  'give $NAME minecraft:iron_pickaxe' \
  'give $NAME minecraft:water_bucket' \
  'give $NAME minecraft:bucket' \
  'give $NAME minecraft:flint_and_steel' \
  'give $NAME minecraft:cobblestone 64' \
  " >/dev/null 2>&1
echo "[ptest] go — follow: tail -f ptest.log ; cast events: sqlite3 .memory-$NAME.db \"select * from events where category='cast'\""
