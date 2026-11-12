#!/usr/bin/env bash
# Gym batch on a named server profile (cycle 6, decision 3): N kitted bots run GYM=$SLUG for TRIALS
# trials each. Gyms run on the local servers; the box (Server B) is for races only.
#
#   PROFILE=local-1 SLUG=portal N=3 TRIALS=4 BIN=target-honest/release/ruststeve BUILD=honest ./gym.sh
#
# Profiles (nothing else in this script names a host or a port):
#   local-1  this Mac, game 25567, RCON 25577, store data/local-1/  (./local-server.sh 1)
#   local-2  this Mac, game 25568, RCON 25578, store data/local-2/  (./local-server.sh 2)
#   box-b    on the OCI box next to Server B, game 25566, RCON 25576, store data/
#
# Rules carried from gym-tonether-b.sh (cycle 2–5): every batch gets a fresh region (GYM_REGION
# auto-increments per profile), every trial writes its gym.db row at start, forceloads start from zero,
# the previous batch's bots get SIGTERM so their trial is recorded `aborted`. New: at most 6 bots per
# server across every pid file of the profile (Server B crashed 4× with 10, 2026-10-05).
# LANDINGS=data/landings-A.json: paired landings (decision 4); bot i runs entries (i-1)*TRIALS onward in order.
set -u
PROFILE=${PROFILE:?set PROFILE=local-1|local-2|box-b}
case "$PROFILE" in
  local-1) MC_HOST=127.0.0.1 MC_PORT=25567 RCON_PORT=25577 STORE=data/local-1 ;;
  local-2) MC_HOST=127.0.0.1 MC_PORT=25568 RCON_PORT=25578 STORE=data/local-2 ;;
  box-b)   MC_HOST=localhost MC_PORT=25566 RCON_PORT=25576 STORE=data ;;
  *) echo "unknown PROFILE $PROFILE" >&2; exit 2 ;;
esac
export RCON_HOST=$MC_HOST RCON_PORT
N=${N:-3}
TRIALS=${TRIALS:-3}
SLUG=${SLUG:-portal}
MAX_BOTS=6
DIR=${DIR:-$(cd "$(dirname "$0")" && pwd)}
BIN=${BIN:-$DIR/target/release/ruststeve}
RCONBIN=${RCONBIN:-$DIR/target/release/rcon}
cd "$DIR" || exit 1
mkdir -p "$STORE" logs/archive pids

IFS=',' read -r -a SLUG_LIST <<< "${SLUGS:-}"
[ -n "${SLUGS:-}" ] && N=${#SLUG_LIST[@]}
PIDFILE=pids/$PROFILE-${PIDTAG:-main}.pids

# SIGTERM the previous batch on this pid file first: the bot records its trial in progress as `aborted`.
if [ -f "$PIDFILE" ]; then
  while read -r p; do kill "$p" 2>/dev/null; done < "$PIDFILE"
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    alive=0
    while read -r p; do kill -0 "$p" 2>/dev/null && alive=1; done < "$PIDFILE"
    [ "$alive" = 0 ] && break
    sleep 1
  done
  while read -r p; do kill -9 "$p" 2>/dev/null; done < "$PIDFILE"
fi
: > "$PIDFILE"
live=0
for f in pids/"$PROFILE"-*.pids; do
  while read -r p; do kill -0 "$p" 2>/dev/null && live=$((live + 1)); done < "$f"
done
if [ $((live + N)) -gt "$MAX_BOTS" ]; then
  echo "refused: $live bots live on $PROFILE + $N new > $MAX_BOTS" >&2
  exit 3
fi

if [ -z "${GYM_REGION:-}" ]; then
  prev=$(cat "$STORE/gym-index" 2>/dev/null || echo 0)
  GYM_REGION=$((prev + 1))
fi
echo "$GYM_REGION" > "$STORE/gym-index"
echo "gym batch: profile=$PROFILE slug=${SLUGS:-$SLUG} N=$N trials=$TRIALS region=$GYM_REGION (x+$((GYM_REGION*3000)), z 3300..4400)"

# KEEP_FORCELOADS=1: a side batch must not clear a running batch's forceloads.
[ "${KEEP_FORCELOADS:-0}" = "1" ] || perl -e 'alarm shift; exec @ARGV' 60 "$RCONBIN" "forceload remove all" "execute in minecraft:the_nether run forceload remove all" "execute in minecraft:the_end run forceload remove all" >/dev/null 2>&1
BUILD=${BUILD:-dev}
WORLD_SEED=$(perl -e 'alarm shift; exec @ARGV' 20 "$RCONBIN" "seed" 2>/dev/null | grep -oE -- "-?[0-9]+" | head -1)
echo "build=$BUILD seed=${WORLD_SEED:-?} bin=$BIN"
for i in $(seq 1 "$N"); do
  name=$(printf 'rust-gym-%03d' "$((i + ${NAME_START:-1} - 1))")  # NAME_START: side batches use 007+
  [ -n "${SLUGS:-}" ] && SLUG=${SLUG_LIST[$((i-1))]}
  [ -f "gym-$name.log" ] && mv "gym-$name.log" "logs/archive/gym-$name.$(date +%m%d-%H%M%S).log"
  rm -f ".memory-$name.db" ".memory-$name.db-shm" ".memory-$name.db-wal" ".frame-$name.txt"
  perl -e 'alarm shift; exec @ARGV' 20 "$RCONBIN" "op $name" >/dev/null 2>&1
  MC_HOST=$MC_HOST MC_PORT=$MC_PORT MC_USERNAME="$name" STEVE_DATA="$DIR/data" \
    RCON_HOST=$MC_HOST RCON_PORT=$RCON_PORT GYM_REGION=$GYM_REGION \
    GYM_DB="$STORE/gym.db" ATTEMPTS_DIR="$STORE/attempts" \
    GYM_LANDINGS="${LANDINGS:-}" GYM_LANDING_OFFSET=$(( (i - 1) * TRIALS )) \
    GYM="$SLUG" GYM_TRIALS="$TRIALS" CRAFT_DEBUG=1 \
    BUILD="$BUILD" WORLD_SEED="${WORLD_SEED:-}" GYM_RUN="gym-$PROFILE-r$GYM_REGION-$SLUG" \
    "$BIN" > "gym-$name.log" 2>&1 &
  echo "$!" >> "$PIDFILE"
  echo "launched $name pid $!"
  sleep "${STAGGER:-4}"
done
echo "all $N launched"
