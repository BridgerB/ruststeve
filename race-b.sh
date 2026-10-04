#!/usr/bin/env bash
# Server-B race. Host-agnostic: runs on the Mac (game port + RCON through SSH tunnels on
# localhost) or on the OCI box next to Server B (localhost directly). Steve races on Server A;
# we never touch it.
#
# Measurement rules (cycle 2, docs/ruststeve-loop-cycle2.md):
#  - every race runs in a FRESH region: lanes at x = 900 + RACE_INDEX*3000 (index persisted in
#    data/race-index), never terrain an earlier race touched;
#  - inventories are cleared at the start (RACE_CLEAR=1 default);
#  - RCON goes through the repo's own `rcon` binary under a hard 60 s timeout with retries —
#    no ssh, so a hung ssh can no longer stall the race (cycle 1 lost 45 min to one);
#  - a relaunched bot is NOT teleported: it reconnects where it was and the step machine resumes;
#  - a watchdog kills (→ relaunches) a bot whose last race.db tick is older than STALE_SECS;
#  - race start, clears, relaunches and watchdog kills are written to race.db (`race_meta`).
set -u

DIR=${DIR:-$(cd "$(dirname "$0")" && pwd)}
BIN=$DIR/target/release/ruststeve
RCONBIN=$DIR/target/release/rcon
DATA=$DIR/data
DB=$DATA/race.db
MC_HOST=${MC_HOST:-localhost}
PORT=${PORT:-25566}
export RCON_HOST=${RCON_HOST:-localhost} RCON_PORT=${RCON_PORT:-25576}
N=${N:-5}
RACE_SECONDS=${RACE_SECONDS:-14400}
RACE_GOAL=${RACE_GOAL:-blaze}
RACE_CLEAR=${RACE_CLEAR:-1}
STALE_SECS=${STALE_SECS:-60}
RUST_VIEW=${RUST_VIEW:-0}
HOLD=45

cd "$DIR" || exit 1

# Fresh region: bump the persisted race index unless RACE_INDEX is given.
if [ -z "${RACE_INDEX:-}" ]; then
  prev=$(cat "$DATA/race-index" 2>/dev/null || echo 0)
  RACE_INDEX=$((prev + 1))
fi
echo "$RACE_INDEX" > "$DATA/race-index"
BASEX=$((900 + RACE_INDEX * 3000))
RACE_ID="race-$(date +%Y%m%d-%H%M%S)-i$RACE_INDEX"

NAMES=(); LANES=()
for i in $(seq 0 $((N-1))); do
  NAMES+=("$(printf 'rust-race-%03d' "$((i+1))")")
  LANES+=($((350 + 90 * i)))
done

# RCON with a hard timeout (perl alarm: portable to macOS and the box) and 3 tries.
rc() {
  local t
  for t in 1 2 3; do
    if perl -e 'alarm shift; exec @ARGV' 60 "$RCONBIN" "$@" >/dev/null 2>&1; then return 0; fi
    sleep 2
  done
  echo "[race-b] rcon FAILED after 3 tries: $*"
  return 1
}
# race.db meta row (the bots create the schema; create `events` defensively if they haven't yet).
meta() {
  local bot=$1 event=$2 detail=${3:-}
  sqlite3 "$DB" "CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY AUTOINCREMENT, ts_ms INTEGER NOT NULL, bot TEXT NOT NULL, category TEXT NOT NULL, event TEXT NOT NULL, step_id TEXT, detail TEXT, x INTEGER, y INTEGER, z INTEGER, health REAL, food REAL, n INTEGER);
    INSERT INTO events(ts_ms,bot,category,event,detail) VALUES ($(date +%s)000,'$bot','race_meta','$event','$(echo "$detail" | tr "'" '"')');" 2>/dev/null
}

# SINGLE-INSTANCE GUARD — kill orphaned race loops + the previous RACE's bots (by pid file, so a
# gym batch running beside the race on the same host is never touched).
for pid in $(pgrep -f 'bash .*race-b.sh'); do [ "$pid" != "$$" ] && kill -9 "$pid" 2>/dev/null; done
[ -f "$DIR/race.pids" ] && while read -r p; do kill -9 "$p" 2>/dev/null; done < "$DIR/race.pids"
: > "$DIR/race.pids"
sleep 2

# Fresh telemetry DB for this race; the previous one is archived, never deleted.
mkdir -p "$DATA/archive"
if [ -f "$DB" ]; then
  sqlite3 "$DB" ".backup '$DATA/archive/race-pre-$RACE_ID.db'" 2>/dev/null
  rm -f "$DB" "$DB-wal" "$DB-shm"
fi

PIDS=()
cleanup() {
  echo "[race-b] cleanup — killing bots"
  for p in "${PIDS[@]:-}"; do kill -9 "$p" 2>/dev/null; done
  rc "forceload remove $((BASEX-20)) 280 $((BASEX+60)) $((LANES[N-1]+40))"
  for c in "${EXTRA_FL[@]:-}"; do [ -n "$c" ] && rc "forceload remove $c"; done
}
EXTRA_FL=()
# INT/TERM must EXIT after cleanup: with a plain trap the loop kept running after `pkill` and
# relaunched every bot the cleanup had just killed (smoke race, 2026-10-02 00:11).
trap cleanup EXIT
trap 'cleanup; trap - EXIT; exit 1' INT TERM

echo "[race-b] $RACE_ID: region x=$BASEX lanes z=${LANES[*]} clear=$RACE_CLEAR host=$MC_HOST:$PORT"
echo "[race-b] phase 1: op bots, forceload the fresh region, keep_inventory"
OPS=(); for n in "${NAMES[@]}"; do OPS+=("op $n"); done
# Start from zero forceloads (leaked by killed gym trials; Server B loads every forced chunk at
# startup and crashed on a > 60 s tick at 01:16 on 2026-10-02 with them piled up).
rc "forceload remove all" "execute in minecraft:the_nether run forceload remove all"
rc "${OPS[@]}" "forceload add $((BASEX-20)) 280 $((BASEX+60)) $((LANES[N-1]+40))" "gamerule keep_inventory true"
sleep 5

echo "[race-b] phase 1.5: clear ghosts"
for n in "${NAMES[@]}"; do rc "kick $n"; done
sleep 4

# Cycle 4 event log (src/learn.rs): rows carry the race id, the build and the world seed.
BUILD=${BUILD:-dev}
WORLD_SEED=$(perl -e 'alarm shift; exec @ARGV' 20 "$RCONBIN" "seed" 2>/dev/null | grep -oE -- "-?[0-9]+" | head -1)
echo "[race-b] build=$BUILD seed=${WORLD_SEED:-?}"
launch_bot() {
  local i=$1
  # The bot enables the viewer when RUST_VIEW is SET at all (even "0"), so only pass it for 1.
  local view=(); [ "$RUST_VIEW" = "1" ] && view=(RUST_VIEW=1)
  env MC_HOST=$MC_HOST MC_PORT=$PORT MC_USERNAME="${NAMES[i]}" STEVE_DATA="$DATA" \
    RCON_HOST=$RCON_HOST RCON_PORT=$RCON_PORT \
    RACE_HOLD=$HOLD RACE_GOAL="$RACE_GOAL" CRAFT_DEBUG=1 ${view[@]+"${view[@]}"} \
    RACE_ID="$RACE_ID" BUILD="$BUILD" WORLD_SEED="${WORLD_SEED:-}" \
    "$BIN" >> "$DIR/race-$i.log" 2>&1 &
  PIDS[$i]=$!
  echo "$!" >> "$DIR/race.pids"
  LAUNCHED[$i]=$SECONDS
}
declare -a LAUNCHED
# Seconds since the bot's tick loop last touched its heartbeat file (999 if missing).
hb_age() { perl -e 'my $m=(stat shift)[9]; print defined $m ? time-$m : 999' "$DIR/.heartbeat-$1"; }

echo "[race-b] phase 2: launching $N bots (hold ${HOLD}s, goal $RACE_GOAL)"
for i in $(seq 0 $((N-1))); do
  : > "$DIR/race-$i.log"
  # Fresh memory per RACE (never per relaunch): POIs from an earlier race are in another region or
  # a deleted world. The smoke race of 2026-10-03 started with 298 iron sightings from race i6 in
  # a world that no longer exists. A relaunch keeps memory: it resumes where it stood.
  rm -f "$DIR/.memory-${NAMES[i]}.db" "$DIR/.memory-${NAMES[i]}.db-wal" "$DIR/.memory-${NAMES[i]}.db-shm" "$DIR/data/.attempt-${NAMES[i]}.json"
  launch_bot "$i"
  sleep 2
done

echo "[race-b] phase 3: waiting for bots to hold, then placing them on the surface of their lanes"
for t in $(seq 1 40); do
  ready=0
  for i in $(seq 0 $((N-1))); do grep -q 'holding' "$DIR/race-$i.log" 2>/dev/null && ready=$((ready+1)); done
  echo "  holding: $ready/$N"
  [ "$ready" -ge "$N" ] && break
  sleep 3
done
# A lane's surface must be LAND: a natural world spawn always is. Race i5 dropped two of five bots
# on the open ocean ("over motion_blocking_no_leaves" is the sea surface there); they floated for
# the whole 4 h, 0/28 steps. Try x offsets inside the forceloaded strip; keep BASEX if all are water.
# Prints "x z" of the first land candidate (x offsets × z ±32, all inside the forceloaded strip).
land_xz() {
  local z0=$1 x z out
  for z in $z0 $((z0+32)) $((z0-32)); do
    for x in $BASEX $((BASEX+16)) $((BASEX+32)) $((BASEX+48)) $((BASEX-16)); do
      out=$(perl -e 'alarm shift; exec @ARGV' 30 "$RCONBIN" "execute positioned $x 0 $z positioned over motion_blocking_no_leaves if block ~ ~-1 ~ minecraft:water" 2>/dev/null)
      case "$out" in *passed*) continue ;; esac
      echo "$x $z"; return
    done
  done
  # Race i6: lanes 3-4 were water at all 15 strip candidates and spent 4 h at sea. Widen along x,
  # outside the strip: forceload the one candidate chunk, wait for it to load (an unloaded column
  # has no heightmap, and an empty test reply would read as "land"), test, and keep the chunk
  # forced only if it is land. The main loop records kept chunks in EXTRA_FL for cleanup (this
  # runs in a subshell, so it cannot set the array itself).
  for x in $((BASEX+128)) $((BASEX-128)) $((BASEX+256)) $((BASEX-256)) $((BASEX+384)) $((BASEX-384)); do
    rc "forceload add $x $z0" >/dev/null
    local loaded=0
    for _ in $(seq 1 20); do
      out=$(perl -e 'alarm shift; exec @ARGV' 30 "$RCONBIN" "execute if loaded $x 0 $z0" 2>/dev/null)
      case "$out" in *passed*) loaded=1; break ;; esac
      sleep 1
    done
    if [ "$loaded" = "1" ]; then
      out=$(perl -e 'alarm shift; exec @ARGV' 30 "$RCONBIN" "execute positioned $x 0 $z0 positioned over motion_blocking_no_leaves if block ~ ~-1 ~ minecraft:water" 2>/dev/null)
      case "$out" in *failed*) echo "$x $z0"; return ;; esac
    fi
    rc "forceload remove $x $z0" >/dev/null
  done
  echo "[race-b] lane z=$z0: water at every candidate — keeping ($BASEX,$z0)" >&2
  echo "$BASEX $z0"
}
declare -a LANDX LANDZ DEATHS_SEEN
for i in $(seq 0 $((N-1))); do
  n=${NAMES[i]}; z=${LANES[i]}
  # Cycle 5: the gym's RandomSurface rules by RCON (scripts/race-place.ts): wait for the column to
  # LOAD before testing it, reject water/lava underfoot and any water within 3. The old land_xz tested
  # unloaded columns, whose empty replies read as land: two of five bots started at sea in i5 and i6.
  if out=$(node "$DIR/scripts/race-place.ts" "$BASEX" "$z" "$RCONBIN" 2>>"$DIR/race-place.log"); then
    read -r lx lz <<<"$out"
  else
    echo "[race-b] lane $i: no land near ($BASEX,$z) — falling back to land_xz" | tee -a "$DIR/race-place.log"
    read -r lx lz < <(land_xz "$z")
  fi
  [ "$lx $lz" != "$BASEX $z" ] && echo "[race-b] lane $i: placed on land at ($lx,$lz) (lane start ($BASEX,$z))"
  meta "$n" placement "lane $i landing ($lx,$lz), lane start ($BASEX,$z)"
  z=$lz
  LANDX[$i]=$lx; LANDZ[$i]=$lz; DEATHS_SEEN[$i]=0
  EXTRA_FL+=("$lx $lz")
  # Surface of a fresh, forceloaded column (no hard-coded y: terrain differs per region).
  PLACE=("execute positioned $lx 0 $z positioned over motion_blocking_no_leaves run tp $n ~ ~1 ~"
         "execute positioned $lx 0 $z positioned over motion_blocking_no_leaves run spawnpoint $n ~ ~1 ~")
  [ "$RACE_CLEAR" = "1" ] && PLACE+=("clear $n")
  rc "${PLACE[@]}"; sleep 1; rc "${PLACE[@]}"
  [ "$RACE_CLEAR" = "1" ] && meta "$n" clear "inventory cleared at race start"
done
meta race start "id=$RACE_ID index=$RACE_INDEX basex=$BASEX lanes=${LANES[*]} n=$N clear=$RACE_CLEAR host=$MC_HOST secs=$RACE_SECONDS"
echo "[race-b] bots positioned + spawnpoints set"

echo "[race-b] phase 4: racing (max ${RACE_SECONDS}s)"
SECONDS=0
declare -a ANNOUNCED
while [ $SECONDS -lt $RACE_SECONDS ]; do
  now_ms=$(($(date +%s) * 1000))
  for i in $(seq 0 $((N-1))); do
    n=${NAMES[i]}
    if [ -z "${ANNOUNCED[i]:-}" ] && [ -n "$(sqlite3 "$DB" "SELECT 1 FROM events WHERE category='win' AND bot='$n' LIMIT 1" 2>/dev/null)" ]; then
      ANNOUNCED[$i]=1; echo "[race-b t=${SECONDS}s] $n reached goal '$RACE_GOAL' — continuing"
    fi
    # Respawn check (cycle 5): after a death the bot must be back within 32 blocks of its landing (the
    # spawnpoint is not always honoured; steve saw the same). If not, tp it back and log a harness event.
    deaths=$(sqlite3 "$DB" "SELECT COUNT(*) FROM events WHERE category='death' AND bot='$n'" 2>/dev/null || echo 0)
    if [ "${deaths:-0}" -gt "${DEATHS_SEEN[i]:-0}" ]; then
      DEATHS_SEEN[$i]=$deaths
      pos=$(perl -e 'alarm shift; exec @ARGV' 30 "$RCONBIN" "data get entity $n Pos" 2>/dev/null | grep -oE -- '-?[0-9]+\.[0-9]+d' | tr -d d | head -3 | tr '\n' ' ')
      read -r px _ pz <<<"$pos"
      if [ -n "${px:-}" ]; then
        d=$(awk -v a="$px" -v b="$pz" -v c="${LANDX[i]}" -v e="${LANDZ[i]}" 'BEGIN { printf "%d", sqrt((a-c)^2 + (b-e)^2) }')
        if [ "$d" -gt 32 ]; then
          echo "[race-b t=${SECONDS}s] HARNESS $n respawned ${d} blocks from its landing — tp back"
          meta "$n" harness_respawn_far "death #$deaths: respawn ${d} blocks from landing (${LANDX[i]},${LANDZ[i]}); tp back"
          rc "execute positioned ${LANDX[i]} 0 ${LANDZ[i]} positioned over motion_blocking_no_leaves run tp $n ~ ~1 ~"
        fi
      fi
    fi
    if kill -0 "${PIDS[i]}" 2>/dev/null; then
      # Watchdog: the process is alive but its TICK LOOP is not running (heartbeat file older
      # than STALE_SECS) → kill; the next pass relaunches it. race.db ticks are per STEP and can
      # be minutes apart, so they are not a liveness signal. 90 s grace after every launch
      # covers the 45 s hold + connect.
      age=$(hb_age "$n")
      if [ $((SECONDS - ${LAUNCHED[i]:-0})) -gt 90 ] && [ "$age" -gt "$STALE_SECS" ]; then
        echo "[race-b t=${SECONDS}s] watchdog: $n heartbeat ${age}s old — killing for relaunch"
        meta "$n" watchdog_kill "heartbeat ${age}s old"
        kill -9 "${PIDS[i]}" 2>/dev/null
      fi
    else
      echo "[race-b t=${SECONDS}s] lane $i ($n) exited — relaunching in place (no tp)"
      meta "$n" relaunch "process exited; resumes where it stands"
      rc "op $n"
      launch_bot "$i"
    fi
  done
  printf '[race-b t=%ds]' "$SECONDS"
  for i in $(seq 0 $((N-1))); do
    st=$(sqlite3 "$DB" "SELECT step_id||'@'||y FROM ticks WHERE bot='${NAMES[i]}' ORDER BY ts_ms DESC LIMIT 1" 2>/dev/null)
    printf ' %02d:%s' "$i" "${st:-?}"
  done
  echo
  sleep 20
done

meta race end "id=$RACE_ID ran ${RACE_SECONDS}s"
rc "say RACE OVER ($RACE_ID, ${RACE_SECONDS}s)"
echo "[race-b] done — rate report:"
"$DIR/scripts/rate-report.sh" "$DB" 2>/dev/null || true
