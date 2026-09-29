#!/bin/bash
# Keep 3 spawn→nether race bots continuously alive for the /loop portal work.
# Relaunches any that died (tunnel "Broken pipe" flakes, or the bot's own
# stuck-detection self-terminate). Does NOT touch the world; localhost tunnel.
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
cd "$DIR" || exit 1
while true; do
  idx=0
  for name in rust-race-011 rust-race-012 rust-race-013; do
    # [M] bracket trick: the bot's env has MC_USERNAME=..., but this avoids the grep
    # command line matching ITSELF (which made the watchdog think dead bots were alive).
    if ! ps ewwx | grep -q "[M]C_USERNAME=$name"; then
      echo "$(date +%H:%M:%S) relaunching $name" >> /tmp/loop-watchdog.log
      MC_HOST=localhost MC_USERNAME="$name" STEVE_DATA=data RACE_GOAL=nether CRAFT_DEBUG=1 \
        RUST_VIEW=1 RUST_VIEW_ASSETS="$DIR/viewer/static/assets.json" \
        nohup "$DIR/target/release/ruststeve" >>"/tmp/loop-race-$idx.log" 2>&1 &
    fi
    idx=$((idx+1))
  done
  sleep 30
done
