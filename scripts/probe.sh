#!/usr/bin/env bash
# probe.sh X Y Z [X Y Z ...] — ground-truth block name(s) from Server B via RCON.
# Vanilla has no "what block is here" command, so each position is tested against a
# candidate list with `execute if block` in ONE rcon session; prints the first match
# (or `?` if none of the candidates match — extend CANDIDATES).
#   RCON_PORT=25576 (Server B tunnel) by default. Needs target/release/rcon.
set -u
DIR=$(cd "$(dirname "$0")/.." && pwd)
RCON=$DIR/target/release/rcon
CANDIDATES=(air cave_air water lava obsidian cobblestone cobbled_deepslate stone deepslate dirt gravel sand
  tuff andesite granite diorite calcite oak_log oak_leaves crafting_table furnace iron_ore deepslate_iron_ore
  coal_ore deepslate_coal_ore copper_ore lapis_ore deepslate_lapis_ore redstone_ore nether_portal fire
  netherrack nether_bricks nether_brick_fence magma_block blackstone basalt soul_sand soul_soil glowstone
  spawner bedrock dripstone_block pointed_dripstone clay grass_block snow ice packed_ice moss_block)
[ $# -lt 3 ] && { echo "usage: probe.sh X Y Z [X Y Z ...]"; exit 2; }
[ -x "$RCON" ] || { echo "build first: cargo build --release --bin rcon"; exit 1; }
while [ $# -ge 3 ]; do
  x=$1; y=$2; z=$3; shift 3
  # Unloaded chunk → "That position is not loaded": forceload it for the probe, then release.
  # DIM=minecraft:the_nether probes the nether (default overworld).
  D=${DIM:-minecraft:overworld}
  cmds=("execute in $D run forceload add $x $z $x $z")
  for c in "${CANDIDATES[@]}"; do cmds+=("execute in $D run execute if block $x $y $z minecraft:$c"); done
  cmds+=("execute in $D run forceload remove $x $z $x $z")
  out=$("$RCON" "${cmds[@]}" | sed '1d;$d')
  name="?"
  case "$out" in *"not loaded"*) name="(chunk not loaded)";; esac
  i=0
  while IFS= read -r line; do
    case "$line" in *passed*) name=${CANDIDATES[$i]}; break;; esac
    i=$((i+1))
  done <<< "$out"
  echo "$x $y $z $name"
done
