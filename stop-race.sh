#!/usr/bin/env bash
# Hard-stop EVERYTHING race-related, in the right order, and clear bot names on the
# server. Use this instead of `pkill -9 race-loop` (which orphans the child race.sh
# launcher — the cause of the duplicate-login / operator-kick churn).
HOST=144.24.32.76
M="sudo /nix/store/4g0rhv7ahr8x14p3zvjk7a9y2dxq1pbg-mcrcon-0.7.2/bin/mcrcon -H localhost -P 25575 -p minecraft-test-rcon"
for pid in $(pgrep -f 'race-loop.sh') $(pgrep -f 'bash .*race.sh') $(pgrep -f gen_dashboard.sh); do kill -9 "$pid" 2>/dev/null; done
sleep 1
for pid in $(pgrep -f 'target/release/ruststeve'); do kill -9 "$pid" 2>/dev/null; done
sleep 2
echo "launchers: $(pgrep -f 'race.*\.sh' | grep -v $$ | wc -l | tr -d ' ') | bots: $(pgrep -f 'target/release/ruststeve' | wc -l | tr -d ' ')"
ssh -o ConnectTimeout=15 bridger@$HOST "$M 'kick race-00' 'kick race-01' 'kick race-02' 'kick race-03' 'kick race-04'" >/dev/null 2>&1
echo "stopped + bot names kicked from server"
