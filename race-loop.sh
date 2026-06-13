#!/usr/bin/env bash
# Run race.sh round after round until SOME bot crafts an iron pickaxe (the goal),
# so the overnight pursuit keeps going unattended. Each round relaunches the bots
# from the freshly-built binary, so code fixes land on the next round automatically.
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
cd "$DIR" || exit 1
round=0
while true; do
  # Already won in a previous round? stop.
  if grep -lq 'RACE GOAL REACHED' race-[0-9].log 2>/dev/null; then
    echo "[loop] iron pickaxe already achieved — stopping"
    break
  fi
  round=$((round + 1))
  echo "[loop] ===== starting race round $round at $(date +%H:%M:%S) ====="
  ./race.sh >> race-orchestrator.log 2>&1
  echo "[loop] round $round ended at $(date +%H:%M:%S)"
  # Did this round win?
  if grep -lq 'RACE GOAL REACHED' race-[0-9].log 2>/dev/null; then
    echo "[loop] 🏆 IRON PICKAXE achieved in round $round — stopping"
    break
  fi
  sleep 5
done
