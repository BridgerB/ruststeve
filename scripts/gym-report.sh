#!/usr/bin/env bash
# gym-report.sh [slug=portal] [from_id=0] — rate table for gym trials from data/gym.db.
# One row per LAUNCH + an aggregate (trials, passes, rate, median/p90 pass time, fail reasons).
# Every launch counts: a row left at outcome='running' longer than 50 min is a KILLED trial and
# counts as a failure; a 'running' row younger than that is in progress and is left out.
set -u
DIR=$(cd "$(dirname "$0")/.." && pwd)
DB=${GYM_DB:-$DIR/data/gym.db}
SLUG=${1:-portal}; FROM=${2:-0}
sqlite3 "$DB" "ALTER TABLE gym_runs ADD COLUMN outcome TEXT" 2>/dev/null   # pre-cycle-2 dbs
NOW=$(($(date +%s) * 1000))
LIVE="(COALESCE(outcome,'')='running' AND ts > $NOW - 3000000)"
OUT="CASE WHEN COALESCE(outcome,'')='running' THEN CASE WHEN $LIVE THEN 'live' ELSE 'killed' END ELSE COALESCE(outcome, CASE WHEN pass=1 THEN 'pass' ELSE 'fail' END) END"
echo "== gym '$SLUG' launches (id >= $FROM) =="
sqlite3 -header -column "$DB" "
  SELECT id, datetime(ts/1000,'unixepoch','localtime') AS at, $OUT AS outcome,
         duration_ms/1000 AS secs, x||','||y||','||z AS spawn, substr(message,1,130) AS message
  FROM gym_runs WHERE slug='$SLUG' AND id >= $FROM ORDER BY id"
echo
echo "== aggregate (live trials excluded; killed = fail) =="
sqlite3 -header -column "$DB" "
  WITH r AS (SELECT * FROM gym_runs WHERE slug='$SLUG' AND id >= $FROM AND NOT $LIVE),
       p AS (SELECT duration_ms/1000 AS s, ROW_NUMBER() OVER (ORDER BY duration_ms) AS rn,
                    COUNT(*) OVER () AS n FROM r WHERE pass=1)
  SELECT (SELECT COUNT(*) FROM r) AS trials,
         (SELECT COALESCE(SUM(pass),0) FROM r) AS passes,
         (SELECT COUNT(*) FROM r WHERE COALESCE(outcome,'')='running') AS killed,
         -- Wilson 95% interval, never a bare fraction (cycle 4, Part 5.1).
         (SELECT CASE WHEN n = 0 THEN 'n/a' ELSE printf('%.0f%% [%.0f%%, %.0f%%]', 100.0*k/n,
            100.0*MAX(0,((1.0*k/n + 3.8416/(2*n)) - 1.96*sqrt((1.0*k/n)*(1-1.0*k/n)/n + 3.8416/(4.0*n*n)))/(1 + 3.8416/n)),
            100.0*MIN(1,((1.0*k/n + 3.8416/(2*n)) + 1.96*sqrt((1.0*k/n)*(1-1.0*k/n)/n + 3.8416/(4.0*n*n)))/(1 + 3.8416/n))) END
          FROM (SELECT COALESCE(SUM(pass),0) AS k, COUNT(*) AS n FROM r)) AS rate_wilson95,
         (SELECT s FROM p WHERE rn = (n+1)/2) AS median_pass_s,
         (SELECT s FROM p WHERE rn = MAX(1, CAST(0.9*n + 0.999 AS INT))) AS p90_pass_s"
echo
echo "== fail messages =="
sqlite3 -header -column "$DB" "
  SELECT COUNT(*) AS n, CASE WHEN COALESCE(outcome,'')='running' THEN '(killed before finishing)'
         WHEN instr(message,'| last:')>0 THEN substr(message, instr(message,'| last:')+8, 70) ELSE substr(message,1,70) END AS message
  FROM gym_runs WHERE slug='$SLUG' AND id >= $FROM AND pass=0 AND NOT $LIVE GROUP BY 2 ORDER BY 1 DESC"
