#!/usr/bin/env bash
# snapshot.sh <bot> [radius_xz=10] [radius_y=5] — freeze a stuck natural state so it can be replayed.
#
# Reads the bot's last position + inventory from data/race.db, CLONEs the world region around it
# into a reserved snapshot slot on Server B (exact, server-side, seconds), and writes
# snapshots/<bot>-<ts>.env with everything gym-from-snapshot.sh needs (source/dest corners, bot
# offset inside the region, translated frame anchor, inventory). Never modifies the source region.
#
# Slot layout: slots are 64 blocks apart along +X at SNAP_BASE_X/Z (far from the x≈900 lanes),
# same Y as the source (the cast logic keys on absolute Y bands, e.g. lava at y≈-54).
set -u
DIR=$(cd "$(dirname "$0")/.." && pwd)
RCON=$DIR/target/release/rcon
DB=${RACE_DB:-$DIR/data/race.db}
BOT=${1:?usage: snapshot.sh <bot> [radius_xz] [radius_y]}
RXZ=${2:-10}; RY=${3:-5}
SNAP_BASE_X=${SNAP_BASE_X:-1500}; SNAP_BASE_Z=${SNAP_BASE_Z:-1500}
mkdir -p "$DIR/snapshots"
# Slot index from a counter file (each snapshot gets its own 64-wide column).
CNT=$DIR/snapshots/.slot
SLOT=$(( $(cat "$CNT" 2>/dev/null || echo 0) ))
echo $((SLOT+1)) > "$CNT"
DX=$((SNAP_BASE_X + SLOT*64)); DZ=$SNAP_BASE_Z

# --at X Y Z: snapshot a region by coordinates (no race.db row; a race-like portal kit is assumed).
if [ "$BOT" = "--at" ]; then
  BX=${2:?X}; BY=${3:?Y}; BZ=${4:?Z}; BOT=${5:-site}; RXZ=${6:-10}; RY=${7:-5}
  row="$BX $BY $BZ build_nether_portal 3 0 0 0 0 128 0 0 0 0 16 0 3 1 0 1"
else
  row=$(sqlite3 -separator ' ' "$DB" "SELECT x,y,z,step_id,pick,sword,logs,planks,sticks,cobblestone,dirt,coal,iron_ore,iron_ingots,food_items,crafting_tables,buckets,water_buckets,flint,flint_and_steel FROM ticks WHERE bot='$BOT' ORDER BY ts_ms DESC LIMIT 1")
fi
[ -z "$row" ] && { echo "no ticks for $BOT in $DB"; exit 1; }
read -r BX BY BZ STEP PICK SWORD LOGS PLANKS STICKS COBBLE DIRT COAL IRON_ORE INGOTS FOOD TABLES BUCKETS WBUCKETS FLINT FNS <<< "$row"
X0=$((BX-RXZ)); Y0=$((BY-RY)); Z0=$((BZ-RXZ)); X1=$((BX+RXZ)); Y1=$((BY+RY)); Z1=$((BZ+RXZ))
# Bot offset inside the region → same offset in the slot.
OX=$((BX-X0)); OY=$((BY-Y0)); OZ=$((BZ-Z0))
ANCHOR=$(cat "$DIR/.frame-$BOT.txt" 2>/dev/null || true)
TANCHOR=""
if [ -n "$ANCHOR" ]; then
  read -r AX AY AZ <<< "$ANCHOR"
  TANCHOR="$((DX + AX - X0)) $((Y0 + AY - Y0)) $((DZ + AZ - Z0))"
fi
TS=$(date +%Y%m%d-%H%M%S)
OUT=$DIR/snapshots/$BOT-$TS.env
echo "[snapshot] $BOT at ($BX,$BY,$BZ) step=$STEP → region ($X0,$Y0,$Z0)-($X1,$Y1,$Z1) → slot $SLOT at ($DX,$Y0,$DZ)"
# DIM=minecraft:the_nether snapshots nether regions (clone is within one dimension; the slot is
# in the same dimension as the source).
D=${DIM:-minecraft:overworld}
"$RCON" "execute in $D run forceload add $((X0-1)) $((Z0-1)) $((X1+1)) $((Z1+1))" \
        "execute in $D run forceload add $((DX-1)) $((DZ-1)) $((DX+2*RXZ+1)) $((DZ+2*RXZ+1))" \
        "execute in $D run clone $X0 $Y0 $Z0 $X1 $Y1 $Z1 $DX $Y0 $DZ replace force"

cat > "$OUT" <<EOF
# snapshot of $BOT at $TS
BOT=$BOT
DIM=$D
STEP=$STEP
SRC="$X0 $Y0 $Z0 $X1 $Y1 $Z1"
DST="$DX $Y0 $DZ"
SLOT=$SLOT
BOT_SRC="$BX $BY $BZ"
BOT_DST="$((DX+OX)) $((Y0+OY)) $((DZ+OZ))"
ANCHOR_SRC="$ANCHOR"
ANCHOR_DST="$TANCHOR"
PICK=$PICK
SWORD=$SWORD
LOGS=$LOGS
PLANKS=$PLANKS
STICKS=$STICKS
COBBLE=$COBBLE
DIRT=$DIRT
COAL=$COAL
IRON_ORE=$IRON_ORE
INGOTS=$INGOTS
FOOD=$FOOD
TABLES=$TABLES
BUCKETS=$BUCKETS
WBUCKETS=$WBUCKETS
FLINT=$FLINT
FNS=$FNS
EOF
echo "[snapshot] wrote $OUT"
