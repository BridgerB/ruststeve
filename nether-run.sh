#!/usr/bin/env bash
# Post-nether milestone harness: drop kitted bot(s) into the nether and drive
# find_fortress -> kill_blaze until they hold a blaze rod (RACE_GOAL=blaze).
#
# Per bot: launch -> wait until online -> op/clear/kit -> RCON-locate a real
# fortress -> build a small netherrack launch pad ~120 blocks WEST of it -> tp the
# bot onto the pad. The bot's find_fortress then sweeps +X into the fortress and
# kill_blaze camps the blaze spawner. A watchdog re-kits + re-tps bots that die.
# Never wipes the world; forceloads only its own regions. keep_inventory (snake_case).
set -u

HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
MCRCON="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
N=${N:-1}
PAD_Y=${PAD_Y:-64}          # nether launch-pad Y (fortresses sit ~y30-90; 64 is a safe mid)
WEST=${WEST:-40}            # how far WEST of the fortress to drop (bot sees brick on arrival + navigates in)
rcon() { $SSH "$MCRCON $*" 2>&1; }

cd "$DIR" || exit 1

# Stop the old spawn->nether race + its watchdog first (focus is post-nether now).
for pid in $(pgrep -f 'loop-watchdog.sh'); do kill -9 "$pid" 2>/dev/null; done
for pid in $(pgrep -f 'bash .*nether-run.sh'); do [ "$pid" != "$$" ] && kill -9 "$pid" 2>/dev/null; done
pkill -9 -f 'target/release/ruststeve' 2>/dev/null
sleep 2

NAMES=(); for i in $(seq 0 $((N-1))); do NAMES+=("$(printf 'rust-race-%03d' "$((i+1))")"); done

# The combat kit given to a bot once it is online (blazes throw fire — armor + shield
# are the difference between a kill and a burn-death; cobble bridges lava while wandering).
kit() {
  local n="$1"
  rcon "'clear $n' \
        'give $n minecraft:iron_sword' \
        'give $n minecraft:iron_helmet' 'give $n minecraft:iron_chestplate' \
        'give $n minecraft:iron_leggings' 'give $n minecraft:iron_boots' \
        'give $n minecraft:shield' \
        'give $n minecraft:cooked_porkchop 16' \
        'give $n minecraft:cobblestone 64'" >/dev/null 2>&1
}

# Build a controlled BLAZE ARENA per bot (like the portal gym's generated pools): a clean
# enclosed room with a NETHER-BRICK floor (so find_fortress sees a "fortress") and a real
# BLAZE SPAWNER in the middle (real blazes to fight). Enclosed + low ceiling so blazes stay in
# melee range. Each bot gets its own arena 256 apart so they don't share/fight. Echoes the
# bot's stand coord for NETHER_START. Center: (ARENA_X + 256*idx, ARENA_Y, ARENA_Z).
ARENA_X=${ARENA_X:-2000}; ARENA_Y=${ARENA_Y:-80}; ARENA_Z=${ARENA_Z:-2000}
build_arena() {
  local idx="$1"
  local cx=$((ARENA_X + 256*idx)) cy=$ARENA_Y cz=$ARENA_Z
  # SMALL SEALED box (bot + blazes forced into melee, nothing escapes). Precompute all coords.
  local sx0=$((cx-4)) sy0=$((cy-3)) sz0=$((cz-4)) sx1=$((cx+4)) sy1=$((cy+5)) sz1=$((cz+4))   # solid shell
  local ax0=$((cx-2)) ay0=$cy      az0=$((cz-2)) ax1=$((cx+2)) ay1=$((cy+2)) az1=$((cz+2))     # 5x3x5 interior
  local fy=$((cy-1))                                                                            # brick floor level
  local standx=$((cx+1)) standz=$((cz+1))                                                       # bot stands beside spawner
  # forceload IN THE NETHER (else the arena chunks aren't loaded and the fills silently no-op —
  # the bot then falls through a missing floor), then shell/interior/floor.
  rcon "'execute in minecraft:the_nether run forceload add $sx0 $sz0 $sx1 $sz1' \
        'execute in minecraft:the_nether run fill $sx0 $sy0 $sz0 $sx1 $sy1 $sz1 minecraft:netherrack' \
        'execute in minecraft:the_nether run fill $ax0 $ay0 $az0 $ax1 $ay1 $az1 minecraft:air' \
        'execute in minecraft:the_nether run fill $ax0 $fy $az0 $ax1 $fy $az1 minecraft:nether_bricks'" >/dev/null 2>&1
  set_spawner "$cx" "$cy" "$cz"
  echo "$standx $cy $standz"
}

# setblock a blaze spawner at $1 $2 $3 — NBT quoting is fragile through ssh→mcrcon, so use the
# triple-escaped form (id:"minecraft:blaze") that survives both shell layers. Tight spawn: blazes
# appear right at the spawner, next to the bot, in the sealed box.
set_spawner() {
  $SSH "$MCRCON \"execute in minecraft:the_nether run setblock $1 $2 $3 minecraft:spawner{SpawnData:{entity:{id:\\\"minecraft:blaze\\\"}},SpawnCount:2,MaxNearbyEntities:4,RequiredPlayerRange:16,SpawnRange:1,MinSpawnDelay:60,MaxSpawnDelay:120}\"" >/dev/null 2>&1
}

online() { rcon "'list'" | grep -q "$1"; }

launch() {  # $1=name $2=idx $3=stand coord "x y z"
  local n="$1" idx="$2" pad="$3"
  MC_HOST=localhost MC_USERNAME="$n" STEVE_DATA=data RACE_GOAL=blaze CRAFT_DEBUG=1 \
    NETHER_START="$pad" \
    RUST_VIEW=1 RUST_VIEW_ASSETS="$DIR/viewer/static/assets.json" \
    nohup "$BIN" >>"/tmp/nether-$idx.log" 2>&1 &
}

# Bring one bot up: build its arena, launch it onto the stand, kit it once settled.
bring_up() {
  local n="$1" idx="$2" stand
  stand=$(build_arena "$idx")
  echo "[nether] $n → blaze arena stand ($stand)"
  launch "$n" "$idx" "$stand"
  for _ in $(seq 1 30); do online "$n" && break; sleep 2; done
  # Settle past "died on join" + the NETHER_START tp before kitting (a kit mid-respawn is lost),
  # then kit twice for good measure.
  sleep 8; kit "$n"; sleep 2; kit "$n"
}

echo "[nether] op + keep_inventory + difficulty normal (blazes need non-Peaceful)"
OPS=""; for n in "${NAMES[@]}"; do OPS+=" 'op $n'"; done
rcon "$OPS 'gamerule keep_inventory true' 'difficulty normal'" >/dev/null 2>&1
sleep 2

# Phase 1: locate a fortress, drop each kitted bot onto a pad west of it.
for i in $(seq 0 $((N-1))); do
  : > "/tmp/nether-$i.log"
  bring_up "${NAMES[$i]}" "$i"
done

# Phase 2: watchdog — relaunch + re-pad + re-kit any bot that dies/disconnects.
echo "[nether] watchdog running (Ctrl-C to stop)"
while true; do
  for i in $(seq 0 $((N-1))); do
    n="${NAMES[$i]}"
    if ! ps ewwx | grep -q "[M]C_USERNAME=$n"; then
      echo "$(date +%H:%M:%S) relaunching $n" >> /tmp/nether-watchdog.log
      bring_up "$n" "$i"
    fi
  done
  sleep 30
done
