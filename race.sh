#!/usr/bin/env bash
# 10-bot race to a natural iron pickaxe. Bots are spaced 50 blocks apart heading
# south (+Z) from (0,0): race-00 at z=0, race-01 at z=50, … race-09 at z=450.
# First bot to craft an iron pickaxe wins; otherwise the race ends after 60 min.
set -u

HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
MCRCON="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
DATA=$DIR/../rustcraft/data
N=5
RACE_SECONDS=3600
HOLD=45

NAMES=(); LANES=()
for i in $(seq 0 $((N-1))); do
  NAMES+=("$(printf 'race-%02d' "$i")")
  LANES+=($((100 * i)))
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
$SSH "$MCRCON $OPS \"forceload add -16 -16 16 470\"" >/dev/null 2>&1
sleep 3

SURF_OUT=$($SSH bash -s <<'REMOTE'
M="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
for i in $(seq 0 4); do
  z=$((100 * i)); surf=74
  for y in $(seq 120 -1 55); do
    out=$($M "execute unless block 0 $y $z minecraft:air unless block 0 $y $z #minecraft:leaves unless block 0 $y $z #minecraft:logs unless block 0 $y $z minecraft:water run difficulty" 2>/dev/null)
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

echo "[race] phase 2: launching $N bots (hold ${HOLD}s, goal iron_pickaxe)"
for i in $(seq 0 $((N-1))); do
  : > "$DIR/race-$i.log"
  # CRAFT_DEBUG uses is_ok() (empty still counts as set), so only EXPORT it for
  # the lead bot — verbose craft choreography on race-00, clean logs elsewhere.
  if [ "$i" -eq 0 ]; then
    MC_HOST=$HOST MC_PORT=25565 MC_USERNAME="${NAMES[i]}" STEVE_DATA="$DATA" \
      RACE_HOLD=$HOLD RACE_GOAL=iron_pickaxe CRAFT_DEBUG=1 \
      "$BIN" >> "$DIR/race-$i.log" 2>&1 &
  else
    MC_HOST=$HOST MC_PORT=25565 MC_USERNAME="${NAMES[i]}" STEVE_DATA="$DATA" \
      RACE_HOLD=$HOLD RACE_GOAL=iron_pickaxe \
      "$BIN" >> "$DIR/race-$i.log" 2>&1 &
  fi
  PIDS+=($!)
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
  TP+=" \"tp ${NAMES[i]} 0 $y ${LANES[i]}\" \"spawnpoint ${NAMES[i]} 0 $y ${LANES[i]}\" \"clear ${NAMES[i]}\""
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
  # all bots dead/exited?
  alive=0
  for p in "${PIDS[@]}"; do kill -0 "$p" 2>/dev/null && alive=$((alive+1)); done
  if [ "$alive" -eq 0 ]; then echo "[race] all bots exited"; break; fi
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
  echo "[race] WINNER: ${NAMES[$WINNER]} (lane z=${LANES[$WINNER]}) crafted an iron pickaxe at t=${SECONDS}s"
  $SSH "$MCRCON \"say RACE OVER — ${NAMES[$WINNER]} wins with an iron pickaxe!\"" >/dev/null 2>&1
else
  echo "[race] no winner within ${RACE_SECONDS}s"
  $SSH "$MCRCON \"say RACE OVER — no iron pickaxe in time.\"" >/dev/null 2>&1
fi
echo "[race] done"
