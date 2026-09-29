#!/usr/bin/env bash
# Server-B race (ruststeve's OWN box: 144.24.32.76:25566, RCON 25576, world
# /var/lib/mc-b). Steve races on Server A (25565); we never touch it. Bots race
# spawn → … → nether → fortress → a BLAZE ROD (RACE_GOAL=blaze). Lanes sit in the
# x≈900 forest band (same seed as A, so identical terrain). Dead bots relaunch
# mid-race and resume their server-side inventory. Default N=4, 4h.
set -u

HOST=144.24.32.76
SSH="ssh -o ConnectTimeout=15 bridger@$HOST"
# Server B RCON is 25576 (A is 25575). Same password.
MCRCON="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25576 -p minecraft-test-rcon"
PORT=25566
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
BIN=$DIR/target/release/ruststeve
DATA=$DIR/data
N=${N:-4}
RACE_SECONDS=${RACE_SECONDS:-14400}   # 4 hours
RACE_GOAL=${RACE_GOAL:-blaze}
HOLD=45

BASEX=900
NAMES=(); LANES=()
for i in $(seq 0 $((N-1))); do
  NAMES+=("$(printf 'rust-race-%03d' "$((i+1))")")
  LANES+=($((350 + 90 * i)))
done

cd "$DIR" || exit 1

# SINGLE-INSTANCE GUARD — kill any orphaned race loops + all ruststeve bots first
# (steve on server A is a TS process, not this binary, so it is never touched).
for pid in $(pgrep -f 'race-loop.sh'); do kill -9 "$pid" 2>/dev/null; done
for pid in $(pgrep -f 'bash .*race-b.sh'); do [ "$pid" != "$$" ] && kill -9 "$pid" 2>/dev/null; done
pkill -9 -f 'target/release/ruststeve' 2>/dev/null
sleep 2

# Fresh telemetry DB for this race (source of truth for the dashboard + report).
rm -f "$DIR"/data/race.db "$DIR"/data/race.db-wal "$DIR"/data/race.db-shm 2>/dev/null

PIDS=()
cleanup() {
  echo "[race-b] cleanup — killing bots"
  for p in "${PIDS[@]:-}"; do kill -9 "$p" 2>/dev/null; done
  pkill -9 -f 'target/release/ruststeve' 2>/dev/null
  $SSH "$MCRCON 'forceload remove 880 280 960 1010'" >/dev/null 2>&1
}
trap cleanup EXIT INT TERM

echo "[race-b] phase 1: op bots, forceload, keep_inventory (server B / 25576)"
OPS=""; for n in "${NAMES[@]}"; do OPS+=" \"op $n\""; done
$SSH "$MCRCON $OPS \"forceload add 880 280 960 1010\" \"gamerule keep_inventory true\"" >/dev/null 2>&1
sleep 3

declare -a SURF
for i in $(seq 0 $((N-1))); do SURF[$i]=74; done

echo "[race-b] phase 1.5: clear ghosts + set lane spawnpoints"
PRE=""
for i in $(seq 0 $((N-1))); do
  y=$(( ${SURF[i]} + 1 ))
  PRE+=" \"kick ${NAMES[i]}\" \"spawnpoint ${NAMES[i]} $BASEX $y ${LANES[i]}\""
done
$SSH "$MCRCON $PRE" >/dev/null 2>&1
sleep 6

launch_bot() {
  local i=$1
  $SSH "$MCRCON \"kick ${NAMES[i]}\"" >/dev/null 2>&1
  sleep 2
  MC_HOST=$HOST MC_PORT=$PORT MC_USERNAME="${NAMES[i]}" STEVE_DATA="$DATA" \
    RACE_HOLD=$HOLD RACE_GOAL="$RACE_GOAL" CRAFT_DEBUG=1 \
    RUST_VIEW=1 RUST_VIEW_ASSETS="$DIR/viewer/static/assets.json" \
    "$BIN" >> "$DIR/race-$i.log" 2>&1 &
  PIDS[$i]=$!
}

echo "[race-b] phase 2: launching $N bots (hold ${HOLD}s, goal $RACE_GOAL)"
for i in $(seq 0 $((N-1))); do
  : > "$DIR/race-$i.log"
  launch_bot "$i"
  sleep 2
done

echo "[race-b] phase 3: waiting for bots to hold, then teleporting into lanes"
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
  CLEARCMD=""; [ "${RACE_CLEAR:-0}" = "1" ] && CLEARCMD=" \"clear ${NAMES[i]}\""
  TP+=" \"tp ${NAMES[i]} $BASEX $y ${LANES[i]}\" \"spawnpoint ${NAMES[i]} $BASEX $y ${LANES[i]}\"$CLEARCMD"
done
$SSH "$MCRCON $TP" >/dev/null 2>&1; sleep 2
$SSH "$MCRCON $TP" >/dev/null 2>&1
echo "[race-b] bots positioned + spawnpoints set"

echo "[race-b] phase 4: racing (max ${RACE_SECONDS}s)"
date +%s > /tmp/race-start
SECONDS=0
WINNER=""
while [ $SECONDS -lt $RACE_SECONDS ]; do
  for i in $(seq 0 $((N-1))); do
    won=$(sqlite3 "$DIR/data/race.db" "SELECT 1 FROM events WHERE category='win' AND bot='${NAMES[i]}' LIMIT 1" 2>/dev/null)
    if [ -n "$won" ]; then WINNER=$i; break; fi
  done
  [ -n "$WINNER" ] && break
  alive=0
  for i in $(seq 0 $((N-1))); do
    if kill -0 "${PIDS[i]}" 2>/dev/null; then
      alive=$((alive+1))
    else
      echo "[race-b] lane $i (${NAMES[i]}) exited — relaunching (resumes server-side inventory)"
      launch_bot "$i"
      for t in $(seq 1 20); do grep -q 'holding' "$DIR/race-$i.log" 2>/dev/null && break; sleep 2; done
      y=$(( ${SURF[i]} + 1 ))
      $SSH "$MCRCON \"op ${NAMES[i]}\" \"tp ${NAMES[i]} $BASEX $y ${LANES[i]}\" \"spawnpoint ${NAMES[i]} $BASEX $y ${LANES[i]}\"" >/dev/null 2>&1
      alive=$((alive+1))
    fi
  done
  printf '[race-b t=%ds] alive=%d' "$SECONDS" "$alive"
  for i in $(seq 0 $((N-1))); do
    pick=$(grep -oE 'pick=Some\([A-Za-z]+\)' "$DIR/race-$i.log" 2>/dev/null | tail -1)
    printf ' %s:%s' "$(printf '%02d' "$i")" "${pick:-pick=None}"
  done
  echo
  sleep 20
done

if [ -n "$WINNER" ]; then
  echo "[race-b] WINNER: ${NAMES[$WINNER]} reached goal '$RACE_GOAL' at t=${SECONDS}s"
  $SSH "$MCRCON \"say RACE OVER — ${NAMES[$WINNER]} reached $RACE_GOAL first!\"" >/dev/null 2>&1
else
  echo "[race-b] no winner within ${RACE_SECONDS}s"
  $SSH "$MCRCON \"say RACE OVER — no bot reached $RACE_GOAL in time.\"" >/dev/null 2>&1
fi
echo "[race-b] done"
