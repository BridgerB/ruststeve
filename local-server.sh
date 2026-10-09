#!/usr/bin/env bash
# Local gym server (cycle 6, decision 3): a vanilla 26.1.2 server on this Mac, so gym load can never
# crash Server B on the box and nothing steve runs can crash the gyms. Same jar as
# server/oci/steve/configuration.nix (Mojang piston-data, sha1-checked), Java 25 from nixpkgs, the box's
# JVM flags, the box's server.properties except difficulty=normal and the ports.
#
#   ./local-server.sh            # local-1: game 25567, RCON 25577, world in data/local-world
#   ./local-server.sh 2          # local-2: game 25568, RCON 25578, world in data/local-world-2
#
# Runs in the foreground; launch it detached (setsid/nohup) and stop it with `rcon stop`.
# The local world is ours to reset: delete data/local-world* while the server is down.
set -eu

DIR=$(cd "$(dirname "$0")" && pwd)
N=${1:-1}
case "$N" in
  1) PORT=25567 RCON_PORT=25577 WORLD=$DIR/data/local-world ;;
  2) PORT=25568 RCON_PORT=25578 WORLD=$DIR/data/local-world-2 ;;
  *) echo "usage: $0 [1|2]" >&2; exit 2 ;;
esac
HEAP_MB=${HEAP_MB:-6144}
JAR_URL=https://piston-data.mojang.com/v1/objects/97ccd4c0ed3f81bbb7bfacddd1090b0c56f9bc51/server.jar
JAR_SHA1=97ccd4c0ed3f81bbb7bfacddd1090b0c56f9bc51
JAR=$DIR/data/local-server/server-26.1.2.jar

mkdir -p "$DIR/data/local-server" "$WORLD"
if [ ! -f "$JAR" ] || [ "$(shasum -a 1 "$JAR" | cut -d' ' -f1)" != "$JAR_SHA1" ]; then
  curl -fsSL -o "$JAR.tmp" "$JAR_URL"
  [ "$(shasum -a 1 "$JAR.tmp" | cut -d' ' -f1)" = "$JAR_SHA1" ] || { echo "server.jar sha1 mismatch" >&2; exit 1; }
  mv "$JAR.tmp" "$JAR"
fi
JAVA=${JAVA:-$(nix build --no-link --print-out-paths nixpkgs#jdk25)/bin/java}

cd "$WORLD"
echo "eula=true" > eula.txt
cat > server.properties <<PROPS
max-players=100
online-mode=false
pvp=false
difficulty=normal
gamemode=survival
enable-command-block=true
spawn-protection=0
view-distance=6
simulation-distance=4
server-port=$PORT
server-ip=127.0.0.1
level-seed=typecraft
level-type=minecraft:normal
generate-structures=true
motd=ruststeve gym local-$N (26.1.2)
white-list=false
spawn-monsters=true
spawn-animals=true
spawn-npcs=true
allow-flight=true
rate-limit=0
enable-rcon=true
rcon.password=${RCON_PASS:-minecraft-test-rcon}
rcon.port=$RCON_PORT
broadcast-rcon-to-ops=true
PROPS

exec "$JAVA" -Xms1G -Xmx${HEAP_MB}M -XX:+UseG1GC -XX:+ParallelRefProcEnabled -XX:MaxGCPauseMillis=200 \
  -XX:+UnlockExperimentalVMOptions -XX:+DisableExplicitGC -XX:G1NewSizePercent=30 -XX:G1MaxNewSizePercent=40 \
  -XX:G1HeapRegionSize=8M -XX:G1ReservePercent=20 -XX:G1HeapWastePercent=5 -XX:G1MixedGCCountTarget=4 \
  -XX:InitiatingHeapOccupancyPercent=15 -XX:G1MixedGCLiveThresholdPercent=90 -XX:SurvivorRatio=32 \
  -XX:+PerfDisableSharedMem -XX:MaxTenuringThreshold=1 -jar "$JAR" nogui
