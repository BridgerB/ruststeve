#!/usr/bin/env bash
# Gym batch on SERVER B: N kitted bots run GYM=$SLUG for TRIALS trials each on random surface
# terrain. Host-agnostic (Mac via SSH tunnels on localhost, or the OCI box directly).
# Cycle 2 rules: every batch gets a FRESH region (GYM_REGION auto-increments, persisted in
# data/gym-index) and every launch writes a gym.db row at start (`running`), so a killed trial
# still counts. Never touches Server A.
set -u
N=${N:-3}
TRIALS=${TRIALS:-3}
SLUG=${SLUG:-portal}
DIR=${DIR:-$(cd "$(dirname "$0")" && pwd)}
BIN=${BIN:-$DIR/target/release/ruststeve}  # override to run a gym build beside a live race
MC_HOST=${MC_HOST:-localhost}
cd "$DIR" || exit 1

if [ -z "${GYM_REGION:-}" ]; then
  prev=$(cat data/gym-index 2>/dev/null || echo 0)
  GYM_REGION=$((prev + 1))
fi
echo "$GYM_REGION" > data/gym-index
echo "gym batch: slug=${SLUGS:-$SLUG} N=$( [ -n "${SLUGS:-}" ] && echo "$SLUGS" | tr ',' '\n' | wc -l | tr -d ' ' || echo "$N") trials=$TRIALS region=$GYM_REGION (x+$((GYM_REGION*3000)), z 3300..4400) host=$MC_HOST"

# Forceloads leak from every trial that is killed before its cleanup, and Server B loads every
# forced chunk at startup: after a day of killed batches it crashed (watchdog: a tick > 60 s,
# 01:16) and then sat at "Preparing spawn area 2%" for minutes. Server B is ruststeve's alone,
# so each batch starts from zero forceloads. (A race running at the same time keeps its lanes
# loaded through its players anyway.)
# KEEP_FORCELOADS=1: a side batch (late-game gyms on bots 007+) must not clear a running batch's forceloads.
[ "${KEEP_FORCELOADS:-0}" = "1" ] || perl -e 'alarm shift; exec @ARGV' 60 "$DIR/target/release/rcon" "forceload remove all" "execute in minecraft:the_nether run forceload remove all" >/dev/null 2>&1
# Only the previous GYM batch's bots (pid file) — never a race running on the same host.
# PIDFILE: a side batch keeps its own pid file so it never kills the main batch (and vice versa).
PIDFILE=${PIDFILE:-gym.pids}
# SIGTERM first: the bot records its trial in progress as `aborted` (never left `running`), then
# exits. -9 only for a bot that hasn't exited after 10 s.
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
: > "$PIDFILE"; sleep 1
mkdir -p logs/archive
# Cycle 4 event log (src/learn.rs): every attempt row carries the build, the world seed and the run.
BUILD=${BUILD:-dev}
WORLD_SEED=$(perl -e 'alarm shift; exec @ARGV' 20 "$DIR/target/release/rcon" "seed" 2>/dev/null | grep -oE -- "-?[0-9]+" | head -1)
echo "build=$BUILD seed=${WORLD_SEED:-?}"
# SLUGS="a,b,c" gives bot i its own slug (N defaults to the list length) — one batch covers a
# whole scenario set (e.g. the five water slugs).
IFS=',' read -r -a SLUG_LIST <<< "${SLUGS:-}"
[ -n "${SLUGS:-}" ] && N=${#SLUG_LIST[@]}
for i in $(seq 1 "$N"); do
  name=$(printf 'rust-gym-%03d' "$((i + ${NAME_START:-1} - 1))")  # NAME_START: side batches use 007+
  [ -n "${SLUGS:-}" ] && SLUG=${SLUG_LIST[$((i-1))]}
  [ -f "gym-$name.log" ] && mv "gym-$name.log" "logs/archive/gym-$name.$(date +%m%d-%H%M%S).log"
  rm -f ".memory-$name.db" ".memory-$name.db-shm" ".memory-$name.db-wal" ".frame-$name.txt"
  MC_HOST=$MC_HOST MC_PORT=25566 MC_USERNAME="$name" STEVE_DATA="$DIR/data" \
    RCON_HOST=localhost RCON_PORT=25576 GYM_REGION=$GYM_REGION \
    GYM="$SLUG" GYM_TRIALS="$TRIALS" CRAFT_DEBUG=1 \
    BUILD="$BUILD" WORLD_SEED="${WORLD_SEED:-}" GYM_RUN="gym-r$GYM_REGION-$SLUG" \
    "$BIN" > "gym-$name.log" 2>&1 &
  echo "$!" >> "$PIDFILE"
  echo "launched $name pid $!"
  sleep "${STAGGER:-4}"  # seconds between bot launches (raise on a fresh world)
done
echo "all $N launched"
