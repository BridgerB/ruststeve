#!/usr/bin/env bash
# Self-healing SSH tunnel for the game (25565) + RCON (25575) ports.
# The mobile IP is blocked at the server firewall, so bots reach the box via this tunnel.
while true; do
  ssh -o ServerAliveInterval=10 -o ServerAliveCountMax=3 -o ExitOnForwardFailure=yes \
      -o StrictHostKeyChecking=accept-new -N \
      -L 25565:localhost:25565 -L 25575:localhost:25575 bridger@144.24.32.76
  echo "[tunnel] dropped $(date +%T) — reconnecting in 2s"
  sleep 2
done
