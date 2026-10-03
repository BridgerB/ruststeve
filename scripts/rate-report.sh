#!/usr/bin/env bash
# rate-report.sh [race.db] — the only accepted form of "it works": rate tables from race.db.
#   per bot: time from session start to each milestone, deaths, current/furthest step, time in it
#   aggregate: enter_nether success rate, median / p90 time-to-enter_nether, median time per step
set -u
DIR=$(cd "$(dirname "$0")/.." && pwd)
DB=${1:-$DIR/data/race.db}
[ -f "$DB" ] || { echo "no db at $DB"; exit 1; }
Q() { sqlite3 -header -column "$DB" "$1"; }
echo "== rate report: $DB  ($(date '+%Y-%m-%d %H:%M'))"
echo
echo "-- per bot: furthest step, current step + minutes in it, deaths, bails, wins"
Q "WITH last AS (SELECT bot, MAX(ts_ms) ts FROM ticks GROUP BY bot),
        cur AS (SELECT t.bot, t.step_id, t.dim, t.y FROM ticks t JOIN last l ON l.bot=t.bot AND l.ts=t.ts_ms),
        start AS (SELECT bot, MIN(ts_ms) t0 FROM events WHERE category='session' GROUP BY bot),
        curspan AS (SELECT bot, step_id, MAX(secs) secs FROM (SELECT bot, step_id, secs, end_ms FROM v_step_spans) s
                    WHERE (bot, end_ms) IN (SELECT bot, MAX(end_ms) FROM v_step_spans GROUP BY bot) GROUP BY bot)
   SELECT cur.bot, (SELECT MAX(done)||'/'||MAX(total) FROM ticks t WHERE t.bot=cur.bot) furthest,
          cur.step_id current_step, printf('%.0f', COALESCE(curspan.secs,0)/60.0) min_in_step, cur.dim, cur.y,
          (SELECT COUNT(*) FROM events e WHERE e.bot=cur.bot AND e.category='death') deaths,
          (SELECT COUNT(*) FROM events e WHERE e.bot=cur.bot AND e.event='bail') bails,
          (SELECT COUNT(*) FROM events e WHERE e.bot=cur.bot AND e.category='win') wins,
          printf('%.0f', (cur_ts.ts - start.t0)/60000.0) run_min
   FROM cur JOIN last cur_ts ON cur_ts.bot=cur.bot LEFT JOIN start ON start.bot=cur.bot LEFT JOIN curspan ON curspan.bot=cur.bot
   ORDER BY cur.bot;"
echo
echo "-- per bot: minutes from session start to each milestone (step completed count rising)"
Q "SELECT m.bot, m.done, m.milestone, printf('%.1f', (m.first_ms - s.t0)/60000.0) min_from_start
   FROM v_milestones m JOIN (SELECT bot, MIN(ts_ms) t0 FROM events WHERE category='session' GROUP BY bot) s ON s.bot=m.bot
   ORDER BY m.bot, m.done;"
echo
echo "-- aggregate: enter_nether (dim flip) success rate + median / p90 minutes from start"
Q "WITH start AS (SELECT bot, MIN(ts_ms) t0 FROM events WHERE category='session' GROUP BY bot),
        nether AS (SELECT t.bot, MIN(t.ts_ms) tn FROM ticks t WHERE t.dim='minecraft:the_nether' GROUP BY t.bot),
        mins AS (SELECT n.bot, (n.tn - s.t0)/60000.0 m FROM nether n JOIN start s ON s.bot=n.bot),
        ranked AS (SELECT m, ROW_NUMBER() OVER (ORDER BY m) rn, COUNT(*) OVER () cnt FROM mins)
   SELECT (SELECT COUNT(*) FROM nether) reached, (SELECT COUNT(*) FROM start) bots,
          printf('%.0f%%', 100.0*(SELECT COUNT(*) FROM nether)/MAX(1,(SELECT COUNT(*) FROM start))) rate,
          (SELECT printf('%.1f', m) FROM ranked WHERE rn = (cnt+1)/2) median_min,
          (SELECT printf('%.1f', m) FROM ranked WHERE rn = MAX(1, CAST(0.9*cnt + 0.5 AS INT))) p90_min;"
echo
echo "-- aggregate: median minutes spent per step (across bots, summed per bot first)"
Q "WITH per AS (SELECT bot, step_id, SUM(secs)/60.0 m FROM v_step_spans WHERE step_id!='' GROUP BY bot, step_id),
        ranked AS (SELECT step_id, m, ROW_NUMBER() OVER (PARTITION BY step_id ORDER BY m) rn, COUNT(*) OVER (PARTITION BY step_id) cnt FROM per)
   SELECT step_id, cnt bots, printf('%.1f', m) median_min FROM ranked WHERE rn=(cnt+1)/2
   ORDER BY (SELECT MIN(ts_ms) FROM ticks t WHERE t.step_id=ranked.step_id);"
echo
echo "-- deaths by step + place (top 15)"
Q "SELECT bot, step_id, x, y, z, COUNT(*) n FROM events WHERE category='death' GROUP BY bot, step_id, x, y, z ORDER BY n DESC LIMIT 15;"
echo
echo "-- top failure messages (top 12)"
Q "SELECT bot, step_id, count, substr(message,1,70) message FROM v_top_fails LIMIT 12;"
echo
echo "-- crafting flake frequency (missing crafting ingredient)"
Q "SELECT bot, SUM(count) n FROM counters WHERE event LIKE '%missing crafting ingredient%' GROUP BY bot;"
echo
echo "-- race integrity (cycle 2): start, clears, relaunches, watchdog kills per bot"
Q "SELECT detail AS race FROM events WHERE category='race_meta' AND event='start' ORDER BY id DESC LIMIT 1;"
Q "SELECT bot, SUM(event='clear') clears, SUM(event='relaunch') relaunches, SUM(event='watchdog_kill') watchdog_kills
   FROM events WHERE category='race_meta' AND bot!='race' GROUP BY bot ORDER BY bot;"
echo
echo "-- breath watchdog (cycle 2): pre-emptions and alarms per bot, deaths by cause marker"
Q "SELECT bot, COALESCE(SUM(CASE WHEN event='preempt' THEN n END),0) preempt_jumps, SUM(event='alarm') alarms
   FROM events WHERE category='breath' GROUP BY bot ORDER BY bot;"
