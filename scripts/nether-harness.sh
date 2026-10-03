#!/usr/bin/env bash
# nether-harness.sh [N=1] [SECS=1500] — measure the nether phase on REAL terrain (Server B).
#
# Per bot: build a small netherrack launch pad WEST of a real fortress (found with `locate`),
# launch the bot with NETHER_START (the bot tp's itself cross-dimension + anchors its
# spawnpoint on the pad), kit it, and let the step machine run find_fortress → kill_blaze
# with RACE_GOAL=blaze (records to data/race.db exactly like a race, so rate-report.sh works).
# Measurement only: no relaunch on death (the death is the data) — the log + race.db keep it.
#   env: FORT="x z" to override the fortress, WEST=120 pad distance, PAD_Y=70, KIT_FIRE_RES=1
set -u
DIR=$(cd "$(dirname "$0")/.." && pwd)
RCON=$DIR/target/release/rcon
BIN=$DIR/target/release/ruststeve
N=${1:-1}; SECS=${2:-1500}
WEST=${WEST:-120}; PAD_Y=${PAD_Y:-70}
cd "$DIR" || exit 1

# Fortress: locate from the nether origin the overworld lanes map to (x≈900/8, z≈450/8).
if [ -z "${FORT:-}" ]; then
  loc=$("$RCON" "execute in minecraft:the_nether positioned 112 64 56 run locate structure minecraft:fortress")
  FORT=$(echo "$loc" | sed -nE 's/.*\[(-?[0-9]+), ~, (-?[0-9]+)\].*/\1 \2/p')
fi
read -r FX FZ <<< "$FORT"
[ -z "${FX:-}" ] && { echo "no fortress located: $loc"; exit 1; }
echo "[nether] fortress at ($FX, ~, $FZ); pads $WEST blocks west at y=$PAD_Y"

# Kill previous harness bots by recorded pid (env vars aren't on the command line, so pkill -f
# can't see MC_USERNAME; a blanket ruststeve kill would take the gym bots down too).
if [ -f "$DIR/nether.pids" ]; then
  while read -r p; do kill -9 "$p" 2>/dev/null; done < "$DIR/nether.pids"
  rm -f "$DIR/nether.pids"; sleep 1
fi
# Never run alongside a race: the bot writes the fixed `data/race.db`, so the harness and a race
# would interleave rows (and the old `rm` here would have deleted a race's data).
if pgrep -f 'bash .*race-b.sh' >/dev/null; then echo "[nether] race-b.sh is running — refusing to touch race.db"; exit 1; fi
if [ -f "$DIR/data/race.db" ]; then
  mkdir -p "$DIR/data/archive"
  sqlite3 "$DIR/data/race.db" ".backup '$DIR/data/archive/race-$(date +%Y%m%d-%H%M%S).db'" 2>/dev/null
  rm -f "$DIR"/data/race.db "$DIR"/data/race.db-wal "$DIR"/data/race.db-shm 2>/dev/null
fi
# `difficulty normal` is SERVER-WIDE (blazes need it) — the last time-boxed bot restores peaceful
# (server.properties default; the overworld gym relies on it) so it can't leak past the run again.
"$RCON" "gamerule keep_inventory true" "difficulty normal" >/dev/null

NAMES=(); for i in $(seq 0 $((N-1))); do NAMES+=("rust-nether-$(printf '%03d' "$((i+1))")"); done
kit() {
  local n=$1
  local extra=""
  [ "${KIT_FIRE_RES:-0}" = "1" ] && extra="give $n minecraft:potion[potion_contents=fire_resistance] 2"
  "$RCON" "clear $n" "give $n minecraft:iron_sword" "give $n minecraft:iron_helmet" "give $n minecraft:iron_chestplate" \
    "give $n minecraft:iron_leggings" "give $n minecraft:iron_boots" "give $n minecraft:shield" "give $n minecraft:iron_pickaxe" \
    "give $n minecraft:cooked_porkchop 16" "give $n minecraft:cobblestone 128" ${extra:+"$extra"} >/dev/null
}
for i in $(seq 0 $((N-1))); do
  n=${NAMES[i]}
  px=$((FX - WEST)); pz=$((FZ + 32*i)); py=$PAD_Y
  # Pad: 7x7 netherrack at py-1, air 7x4 above, in a forceloaded nether region.
  # Forceload first and let the chunks actually load (the fill right after a forceload hit
  # "not loaded" and silently left NO pad — run 3 spawned in mid-air), then build + verify.
  "$RCON" "op $n" "execute in minecraft:the_nether run forceload add $((px-24)) $((pz-24)) $((px+24)) $((pz+24))" >/dev/null
  sleep 4
  for try in 1 2 3; do
    "$RCON" "execute in minecraft:the_nether run fill $((px-3)) $((py-1)) $((pz-3)) $((px+3)) $((py-1)) $((pz+3)) minecraft:netherrack" \
            "execute in minecraft:the_nether run fill $((px-3)) $py $((pz-3)) $((px+3)) $((py+3)) $((pz+3)) minecraft:air" >/dev/null
    if "$RCON" "execute in minecraft:the_nether run execute if block $px $((py-1)) $pz minecraft:netherrack" | grep -q passed; then break; fi
    echo "[nether] pad not placed yet (try $try) — waiting for chunks"; sleep 4
  done
  : > "$DIR/nether-$i.log"
  MC_HOST=144.24.32.76 MC_PORT=25566 MC_USERNAME="$n" STEVE_DATA="$DIR/data" RACE_GOAL=blaze CRAFT_DEBUG=1 RACE_HOLD=12 \
    NETHER_START="$px $py $pz" RCON_HOST=localhost RCON_PORT=25576 \
    "$BIN" >> "$DIR/nether-$i.log" 2>&1 &
  echo $! >> "$DIR/nether.pids"
  echo "[nether] $n pid $! → pad ($px,$py,$pz), log nether-$i.log"
  # Time-box the run: the bot itself never exits (it respawns and keeps going).
  ( bp=$!; sleep "$SECS"; kill -9 "$bp" 2>/dev/null && echo "[nether] $n stopped after ${SECS}s" >> "$DIR/nether-$i.log"
    sleep 2
    alive=0; while read -r p; do kill -0 "$p" 2>/dev/null && alive=1; done < "$DIR/nether.pids"
    [ "$alive" = 0 ] && "$RCON" "difficulty peaceful" >/dev/null && echo "[nether] all bots stopped — difficulty restored to peaceful" >> "$DIR/nether-$i.log" ) &
  for t in $(seq 1 30); do grep -q 'nether-start: now at' "$DIR/nether-$i.log" 2>/dev/null && break; sleep 2; done
  sleep 3; kit "$n"; sleep 2; kit "$n"
done
echo "[nether] $N bot(s) running for up to ${SECS}s — watch: tail -f nether-0.log; report: scripts/rate-report.sh"
