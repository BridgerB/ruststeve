#!/usr/bin/env bash
# 5-bot race from spawn to the NETHER (build + light an obsidian-cast portal and
# step through). Bots are spaced 100 blocks apart heading south (+Z) from (0,0):
# race-00 at z=0, race-01 at z=100, … race-04 at z=400. First bot to reach the
# nether wins; otherwise the race ends after RACE_SECONDS (2h). Dead bots are
# relaunched mid-race and resume their server-side inventory.
set -u

HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
MCRCON="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
DATA=$DIR/../rustcraft/data
N=3
RACE_SECONDS=7200
HOLD=45

# Lanes sit in the FORESTED band near the natural world spawn (x≈705) — the old
# x=0 origin was treeless, so bots gathered 0 logs forever. Each lane is a fixed x
# with z spread 80 apart (z=300,380,…) north of the Steve-bot spawn (~z700).
BASEX=680
NAMES=(); LANES=()
for i in $(seq 0 $((N-1))); do
  NAMES+=("$(printf 'race-%02d' "$i")")
  LANES+=($((350 + 120 * i)))
done

cd "$DIR" || exit 1
PIDS=()
cleanup() {
  echo "[race] cleanup — killing bots"
  for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null; done
  pkill -f 'target/release/ruststeve' 2>/dev/null
  $SSH "$MCRCON 'forceload remove all'" >/dev/null 2>&1
}
trap cleanup EXIT

echo "[race] phase 1: op bots, forceload + probe lane surfaces"
OPS=""; for n in "${NAMES[@]}"; do OPS+=" \"op $n\""; done
$SSH "$MCRCON $OPS \"forceload add 685 280 725 640\"" >/dev/null 2>&1
sleep 3

SURF_OUT=$($SSH bash -s <<'REMOTE'
M="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
for i in $(seq 0 2); do
  z=$((350 + 120 * i)); surf=74
  for y in $(seq 120 -1 55); do
    out=$($M "execute unless block 680 $y $z minecraft:air unless block 680 $y $z #minecraft:leaves unless block 680 $y $z #minecraft:logs unless block 680 $y $z minecraft:water run difficulty" 2>/dev/null)
    case "$out" in *ifficulty*) surf=$y; break;; esac
  done
  echo "SURF $i $surf"
done
REMOTE
)
echo "$SURF_OUT"
declare -a SURF
for i in $(seq 0 $((N-1))); do SURF[$i]=74; done
while read -r tag i y; do [ "$tag" = SURF ] && SURF[$i]=$y; done <<< "$SURF_OUT"

# Phase 1.5 — CRITICAL: clear any stale ghost connection for each name and set its
# spawnpoint to the lane BEFORE launching. Two bugs this fixes:
#   1. A lingering ghost with the same name makes the fresh login a duplicate, and the
#      server kicks one of them (BrokenPipe) — the "keeps joining then leaving" churn.
#   2. Without a pre-set spawnpoint the bot spawns at its OLD persistent spawnpoint
#      (often a treeless/deforested spot), starts gathering there, finds no logs, and
#      burns out on long-range pathfinding. Spawning AT the forest lane fixes it.
echo "[race] phase 1.5: clear ghosts + set lane spawnpoints (spawn AT the forest)"
PRE=""
for i in $(seq 0 $((N-1))); do
  y=$(( ${SURF[i]} + 1 ))
  PRE+=" \"kick ${NAMES[i]}\" \"spawnpoint ${NAMES[i]} $BASEX $y ${LANES[i]}\""
done
$SSH "$MCRCON $PRE" >/dev/null 2>&1
sleep 6   # let the server fully drop the kicked ghosts before fresh logins

# Launch one bot process for lane i (verbose CRAFT_DEBUG only on the lead bot).
# Appends to its log (so a restart's output accumulates) and records the PID by index.
# Kicks any lingering ghost of this name first so the fresh login is never a duplicate.
launch_bot() {
  local i=$1
  $SSH "$MCRCON \"kick ${NAMES[i]}\"" >/dev/null 2>&1
  sleep 2
  if [ "$i" -eq 0 ]; then
    MC_HOST=$HOST MC_PORT=25565 MC_USERNAME="${NAMES[i]}" STEVE_DATA="$DATA" \
      RACE_HOLD=$HOLD RACE_GOAL=nether CRAFT_DEBUG=1 \
      "$BIN" >> "$DIR/race-$i.log" 2>&1 &
  else
    MC_HOST=$HOST MC_PORT=25565 MC_USERNAME="${NAMES[i]}" STEVE_DATA="$DATA" \
      RACE_HOLD=$HOLD RACE_GOAL=nether \
      "$BIN" >> "$DIR/race-$i.log" 2>&1 &
  fi
  PIDS[$i]=$!
}

echo "[race] phase 2: launching $N bots (hold ${HOLD}s, goal nether)"
for i in $(seq 0 $((N-1))); do
  : > "$DIR/race-$i.log"
  launch_bot "$i"
  sleep 1
done

echo "[race] phase 3: waiting for bots to hold, then teleporting into lanes"
for t in $(seq 1 40); do
  ready=0
  for i in $(seq 0 $((N-1))); do grep -q 'holding' "$DIR/race-$i.log" 2>/dev/null && ready=$((ready+1)); done
  echo "  holding: $ready/$N"
  [ "$ready" -ge "$N" ] && break
  sleep 3
done
TP=""
for i in $(seq 0 $((N-1))); do
  y=$(( ${SURF[i]} + 1 ))
  # teleport into the lane, set the per-player spawnpoint there (so a death
  # respawns into the lane), and CLEAR the inventory so every bot starts the
  # race from scratch (race-NN players keep items between runs otherwise).
  TP+=" \"tp ${NAMES[i]} $BASEX $y ${LANES[i]}\" \"spawnpoint ${NAMES[i]} $BASEX $y ${LANES[i]}\" \"clear ${NAMES[i]}\""
done
$SSH "$MCRCON $TP" >/dev/null 2>&1; sleep 2
$SSH "$MCRCON $TP" >/dev/null 2>&1
echo "[race] bots positioned + spawnpoints set; lanes z=0..450"

echo "[race] phase 4: racing (max ${RACE_SECONDS}s)"
date +%s > /tmp/race-start   # exact wall-clock race start (for the dashboard's milestone times)
SECONDS=0
WINNER=""
while [ $SECONDS -lt $RACE_SECONDS ]; do
  for i in $(seq 0 $((N-1))); do
    if grep -q 'RACE GOAL REACHED' "$DIR/race-$i.log" 2>/dev/null; then
      WINNER=$i; break
    fi
  done
  [ -n "$WINNER" ] && break
  # Relaunch any bot whose process exited (a server-load disconnect makes the bot
  # exit cleanly via the `?` on wait_ticks). The server PERSISTS the player's
  # inventory across reconnects, so a relaunched bot resumes its progress rather
  # than starting over — re-op + re-tp into its lane (NO clear) and it picks up
  # where it left off. This keeps a 2-hour race populated despite disconnects.
  alive=0
  for i in $(seq 0 $((N-1))); do
    if kill -0 "${PIDS[i]}" 2>/dev/null; then
      alive=$((alive+1))
    else
      echo "[race] lane $i (${NAMES[i]}) exited — relaunching (resumes server-side inventory)"
      launch_bot "$i"
      for t in $(seq 1 20); do grep -q 'holding' "$DIR/race-$i.log" 2>/dev/null && break; sleep 2; done
      y=$(( ${SURF[i]} + 1 ))
      $SSH "$MCRCON \"op ${NAMES[i]}\" \"tp ${NAMES[i]} $BASEX $y ${LANES[i]}\" \"spawnpoint ${NAMES[i]} $BASEX $y ${LANES[i]}\"" >/dev/null 2>&1
      alive=$((alive+1))
    fi
  done
  # status line every loop
  printf '[race t=%ds] alive=%d' "$SECONDS" "$alive"
  for i in $(seq 0 $((N-1))); do
    pick=$(grep -oE 'pick=Some\([A-Za-z]+\)' "$DIR/race-$i.log" 2>/dev/null | tail -1)
    printf ' %s:%s' "$(printf '%02d' "$i")" "${pick:-pick=None}"
  done
  echo
  sleep 20
done

if [ -n "$WINNER" ]; then
  echo "[race] WINNER: ${NAMES[$WINNER]} (lane z=${LANES[$WINNER]}) reached the NETHER at t=${SECONDS}s"
  $SSH "$MCRCON \"say RACE OVER — ${NAMES[$WINNER]} reached the NETHER first!\"" >/dev/null 2>&1
else
  echo "[race] no winner within ${RACE_SECONDS}s"
  $SSH "$MCRCON \"say RACE OVER — no bot reached the nether in time.\"" >/dev/null 2>&1
fi
echo "[race] done"
