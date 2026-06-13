#!/usr/bin/env bash
# Isolation test harness: validate ONE step in isolation across N bots.
#
#   ./isolation-test.sh <step_id> [N] [SECS]
#
# Spawns N bots spread 50 blocks apart, builds each a clean arena, gives it the
# step's prerequisites (items + world setup), then runs the bot in STEVE_TEST mode
# (run only that step until its is_complete passes). Reports PASS/FAIL per bot.
#
# Prerequisites for each step are defined in setup_prereqs() below — add a case to
# support a new step. The bot binary's test mode does the rest (see main.rs).
set -u
STEP="${1:?usage: isolation-test.sh <step_id> [N] [SECS]}"
N="${2:-3}"
SECS="${3:-240}"

HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
MCRCON="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
DATA=$DIR/../rustcraft/data
Y=70                     # bot stands here; arenas are at a fixed Y so terrain is irrelevant
BASEX=400; BASEZ=400     # lane 0 origin; lane i is 50 east
cd "$DIR" || exit 1

rcon() { $SSH "$MCRCON $*" 2>&1; }

# Build a clean arena for one lane: deep solid stone below (so mining works), a flat
# top to stand on, and open air above.
build_arena() {
  local x=$1 z=$2
  # Wide enough EAST (+X) to cover the lava pool AND where the portal frame anchors
  # (~lava+6), so leftover obsidian/cobble/lava from prior runs is wiped — a stale
  # 10-obsidian frame makes the bot skip prepare and fail. 36x25x21=18900 < 32768.
  rcon \
    "'fill $((x-10)) 45 $((z-10)) $((x+25)) $((Y-1)) $((z+10)) minecraft:stone'" \
    "'fill $((x-10)) $Y $((z-10)) $((x+25)) $((Y+12)) $((z+10)) minecraft:air'" >/dev/null
}

# Give items + place world features for STEP at lane (name,x,z). Add a case per step.
setup_prereqs() {
  local name=$1 x=$2 z=$3
  local g="'clear $name' 'give $name minecraft:iron_pickaxe'"   # a baseline tool
  case "$STEP" in
    craft_planks)        g="'clear $name' 'give $name minecraft:oak_log 16'";;
    craft_crafting_table)g="'clear $name' 'give $name minecraft:oak_planks 16'";;
    craft_sticks)        g="'clear $name' 'give $name minecraft:oak_planks 16'";;
    craft_wooden_pickaxe)g="'clear $name' 'give $name minecraft:oak_planks 16' 'give $name minecraft:stick 8'";;
    mine_stone|gather_build_blocks)
                         g="'clear $name' 'give $name minecraft:stone_pickaxe'";;       # stone is the arena floor
    craft_stone_pickaxe) g="'clear $name' 'give $name minecraft:cobblestone 16' 'give $name minecraft:stick 8'";;
    craft_furnace)       g="'clear $name' 'give $name minecraft:cobblestone 32'";;
    mine_coal)           g="'clear $name' 'give $name minecraft:stone_pickaxe'"
                         # seed coal ore in the stone below the bot
                         rcon "'fill $((x-2)) 60 $((z-2)) $((x+2)) 64 $((z+2)) minecraft:coal_ore'" >/dev/null;;
    mine_iron)           g="'clear $name' 'give $name minecraft:stone_pickaxe'"
                         rcon "'fill $((x-2)) 60 $((z-2)) $((x+2)) 64 $((z+2)) minecraft:iron_ore'" >/dev/null;;
    smelt_iron)          g="'clear $name' 'give $name minecraft:furnace' 'give $name minecraft:raw_iron 11' 'give $name minecraft:coal 8'";;
    craft_iron_pickaxe)  g="'clear $name' 'give $name minecraft:iron_ingot 3' 'give $name minecraft:stick 8'";;
    craft_bucket)        g="'clear $name' 'give $name minecraft:iron_pickaxe' 'give $name minecraft:iron_ingot 6'";;
    get_water_buckets)   g="'clear $name' 'give $name minecraft:iron_pickaxe' 'give $name minecraft:bucket 2'"
                         # a contained water pool 3 east
                         rcon "'fill $((x+4)) $Y $((z-1)) $((x+6)) $((Y+1)) $((z+1)) minecraft:stone'" \
                              "'fill $((x+5)) $Y $z $((x+5)) $Y $z minecraft:water'" >/dev/null;;
    get_flint_and_steel) g="'clear $name' 'give $name minecraft:iron_pickaxe' 'give $name minecraft:iron_ingot 2'"
                         # gravel pile to mine flint from
                         rcon "'fill $((x+3)) $Y $((z-2)) $((x+6)) $((Y+2)) $((z+2)) minecraft:gravel'" >/dev/null;;
    build_nether_portal|enter_nether)
                         g="'clear $name' 'give $name minecraft:iron_pickaxe' \
                            'give $name minecraft:water_bucket' 'give $name minecraft:bucket' \
                            'give $name minecraft:flint_and_steel' 'give $name minecraft:cobblestone 128'"
                         # RECESSED lava pit 6 east: carve the floor open (air y69-70)
                         # and put lava one block lower (y68) so the bot's sightline
                         # clears the floor and actually SEES it (flush-with-floor lava
                         # is invisible — the LOS grazes the floor stone).
                         rcon "'fill $((x+4)) $((Y-1)) $((z-5)) $((x+14)) $Y $((z+5)) minecraft:air'" \
                              "'fill $((x+4)) $((Y-2)) $((z-5)) $((x+14)) $((Y-2)) $((z+5)) minecraft:lava'" >/dev/null;;
  esac
  # Table-based crafts need a table reachable (they route through get_crafting_table,
  # which can't make one without planks). Place one next to the bot.
  case "$STEP" in
    craft_planks|craft_sticks|craft_wooden_pickaxe|craft_stone_pickaxe|craft_furnace|craft_iron_pickaxe|craft_bucket|get_flint_and_steel)
      rcon "'setblock $x $Y $((z+1)) minecraft:crafting_table'" >/dev/null;;
  esac
  rcon "$g" >/dev/null
}

echo "[itest] step=$STEP bots=$N secs=$SECS"
pkill -9 -f 'STEVE_TEST=' 2>/dev/null
rcon "'forceload remove all'" >/dev/null
# Forceload all lanes (each lane is 50 apart; cover from base to base+N*50, ±20)
rcon "'forceload add $((BASEX-20)) $((BASEZ-20)) $((BASEX+N*50+20)) $((BASEZ+20))'" >/dev/null
sleep 6

NAMES=(); for i in $(seq 0 $((N-1))); do NAMES+=("itest-$(printf '%02d' "$i")"); done
echo "[itest] building arenas"
for i in $(seq 0 $((N-1))); do build_arena $((BASEX+i*50)) "$BASEZ"; done

echo "[itest] launching bots in STEVE_TEST mode"
for i in $(seq 0 $((N-1))); do
  : > "$DIR/itest-$i.log"
  # CAST_SNIFF uses is_ok() (empty still counts), so only EXPORT it for bot 0 when
  # ITEST_SNIFF is set in the environment.
  if [ "$i" -eq 0 ] && [ -n "${ITEST_SNIFF:-}" ]; then
    STEVE_TEST="$STEP" STEVE_TEST_SECS="$SECS" CRAFT_DEBUG=1 CAST_SNIFF=1 \
      MC_HOST=$HOST MC_PORT=25565 MC_USERNAME="${NAMES[i]}" STEVE_DATA="$DATA" RACE_HOLD=30 \
      "$BIN" >> "$DIR/itest-$i.log" 2>&1 &
  else
    STEVE_TEST="$STEP" STEVE_TEST_SECS="$SECS" CRAFT_DEBUG=1 \
      MC_HOST=$HOST MC_PORT=25565 MC_USERNAME="${NAMES[i]}" STEVE_DATA="$DATA" RACE_HOLD=30 \
      "$BIN" >> "$DIR/itest-$i.log" 2>&1 &
  fi
  sleep 1
done
for t in $(seq 1 25); do
  r=0; for i in $(seq 0 $((N-1))); do grep -q 'holding' "$DIR/itest-$i.log" 2>/dev/null && r=$((r+1)); done
  [ "$r" -ge "$N" ] && break; sleep 2
done

echo "[itest] tp bots into lanes + set prerequisites"
for i in $(seq 0 $((N-1))); do
  x=$((BASEX+i*50))
  rcon "'op ${NAMES[i]}' 'tp ${NAMES[i]} $x $Y $BASEZ' 'spawnpoint ${NAMES[i]} $x $Y $BASEZ'" >/dev/null
  setup_prereqs "${NAMES[i]}" "$x" "$BASEZ"
done

echo "[itest] running (max ${SECS}s)…"
END=$((SECS+40))
for s in $(seq 0 10 $END); do
  done=0
  for i in $(seq 0 $((N-1))); do grep -q 'TEST RESULT:' "$DIR/itest-$i.log" 2>/dev/null && done=$((done+1)); done
  [ "$done" -ge "$N" ] && break
  sleep 10
done

echo ""
echo "===== RESULTS: $STEP ====="
pass=0
for i in $(seq 0 $((N-1))); do
  res=$(grep 'TEST RESULT:' "$DIR/itest-$i.log" 2>/dev/null | tail -1)
  [ -z "$res" ] && res="TEST RESULT: (still running / no result)"
  echo "  ${NAMES[i]}: ${res#TEST RESULT: }"
  echo "$res" | grep -q 'PASS' && pass=$((pass+1))
done
echo "  ----> $pass/$N passed"
pkill -9 -f 'STEVE_TEST=' 2>/dev/null
