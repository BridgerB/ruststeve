#!/usr/bin/env bash
# Live race dashboard → /tmp/race.html (auto-refreshing). Reads the telemetry DB
# data/race.db (the single source of truth) — NO stdout grepping. Milestone times come
# from the `milestone` events (wall-clock), current step/pick/done from the latest `ticks`
# row per bot, the winner from a `win` event.
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
DB="$DIR/data/race.db"
OUT=/tmp/race.html

# The FULL roadmap (spawn → dragon). Rows 0..17 are the implemented steps and map to the
# completed-step count `done` (row idx reached when done ≥ idx+1); rows past 17 are not yet
# implemented and stay ✗ so the board shows the whole plan.
MS_NAMES=("Logs" "Planks" "Table" "Sticks" "Wood Pick" "Cobble" "Stone Pick" "Furnace" "Coal" "Iron Ore" "Smelt" "IRON PICK" \
  "Buckets" "Water Buckets" "Gather Food" "Flint & Steel" "Build Portal" "ENTER NETHER" \
  "Nether Fortress" "Kill Blazes" "Hunt Endermen" "Return Overworld" "Eyes of Ender" "Find Stronghold" \
  "Activate End Portal" "Bow & Arrows" "Enter The End" "End Crystals" "KILL DRAGON")
GOAL_IDX=17
ICON_BASE="https://cdn.jsdelivr.net/gh/InventivetalentDev/minecraft-assets@1.21.1/assets/minecraft/textures"
MS_ICON=(
  "block/oak_log" "block/oak_planks" "block/crafting_table_front" "item/stick"
  "item/wooden_pickaxe" "block/cobblestone" "item/stone_pickaxe" "block/furnace_front"
  "item/coal" "item/raw_iron" "item/iron_ingot" "item/iron_pickaxe"
  "item/bucket" "item/water_bucket" "item/cooked_beef" "item/flint_and_steel"
  "block/obsidian" "block/netherrack" "block/nether_bricks" "item/blaze_rod"
  "item/ender_pearl" "block/grass_block_side" "item/ender_eye" "block/stone_bricks"
  "block/end_portal_frame_top" "item/bow" "block/end_stone" "block/glass" "block/dragon_egg"
)

q() { sqlite3 "$DB" "$1" 2>/dev/null; }

while true; do
  now=$(date +%s); now_ms=$((now * 1000))
  rs=$(cat /tmp/race-start 2>/dev/null); rs=${rs:-$now}
  T=$(( now - rs )); [ "$T" -lt 0 ] && T=0
  tstr=$(printf '%d:%02d' $((T/60)) $((T%60)))

  # Pull everything from the DB in a handful of queries (bash 3.2: no assoc arrays → temp files).
  q "SELECT DISTINCT bot FROM ticks ORDER BY bot" > /tmp/dash_bots.txt
  q "SELECT bot, MIN(ts_ms) FROM events WHERE category='session' GROUP BY bot" > /tmp/dash_start.txt
  q "SELECT bot, done, first_ms FROM v_milestones" | tr '|' ' ' > /tmp/dash_ms.txt
  # latest tick per bot (step_name, pick tier, done, ts_ms)
  q "SELECT t.bot, t.step_name, t.pick, t.done, t.ts_ms FROM ticks t
     JOIN (SELECT bot, MAX(ts_ms) m FROM ticks GROUP BY bot) x ON t.bot=x.bot AND t.ts_ms=x.m" \
     | tr '|' '\t' > /tmp/dash_now.txt
  winbot=$(q "SELECT bot FROM events WHERE category='win' ORDER BY ts_ms LIMIT 1")

  BOTS=($(cat /tmp/dash_bots.txt))   # bot names have no spaces → safe word-split
  nbots=${#BOTS[@]}
  alive=0
  for b in "${BOTS[@]}"; do
    last=$(awk -F'\t' -v b="$b" '$1==b{print $5}' /tmp/dash_now.txt)
    [ -n "$last" ] && [ $(( now_ms - last )) -lt 30000 ] && alive=$((alive+1))
  done
  winner=""; [ -n "$winbot" ] && winner="🏆 WINNER: $winbot"

  PICK_NAME=(None Wood Stone Iron Diamond)
  {
    echo '<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="refresh" content="4">'
    echo '<title>ruststeve race</title><style>'
    echo 'body{background:#0d1117;color:#c9d1d9;font:14px -apple-system,system-ui,sans-serif;padding:22px}'
    echo 'h1{font-size:20px;margin:0 0 4px}.sub{color:#8b949e;margin-bottom:16px}'
    echo 'table{border-collapse:collapse}th,td{border:1px solid #30363d;padding:6px 9px;text-align:center}'
    echo 'th{background:#161b22}td.bot{text-align:left;font-weight:600;white-space:nowrap}'
    echo '.steplabel{text-align:left;font-weight:600;background:#161b22;white-space:nowrap}.botcol{white-space:nowrap}'
    echo '.ic{width:22px;height:22px;vertical-align:middle;image-rendering:pixelated;margin-right:7px}'
    echo '.ok{color:#3fb950;font-weight:700}.no{color:#5a2c2c}'
    echo '.cur{background:#1f6feb22;color:#58a6ff;text-align:left;white-space:nowrap}'
    echo '.idle{color:#8b949e}.goal{background:#3a2d00}.win{color:#d29922;font-weight:700}'
    echo 'small{color:#8b949e}'
    echo '</style></head><body>'
    echo '<h1>🏁 ruststeve &mdash; race to the Nether</h1>'
    echo "<div class=sub>elapsed ${tstr} &nbsp;&bull;&nbsp; alive ${alive}/${nbots} &nbsp;&bull;&nbsp; <span class=win>${winner}</span> &nbsp;<small>(data/race.db)</small></div>"

    echo '<table>'
    # bots across the TOP
    echo '<tr><th class=steplabel>Step</th>'
    for b in "${BOTS[@]}"; do
      last=$(awk -F'\t' -v b="$b" '$1==b{print $5}' /tmp/dash_now.txt)
      idletag=""; { [ -z "$last" ] || [ $(( now_ms - last )) -gt 30000 ]; } && idletag=" <small>(idle)</small>"
      printf '<th class=botcol>%s%s</th>' "$b" "$idletag"
    done
    echo '</tr>'
    # current step per bot
    echo '<tr><td class=steplabel>Now doing</td>'
    for b in "${BOTS[@]}"; do
      step=$(awk -F'\t' -v b="$b" '$1==b{print $2}' /tmp/dash_now.txt)
      pk=$(awk -F'\t' -v b="$b" '$1==b{print $3}' /tmp/dash_now.txt)
      dn=$(awk -F'\t' -v b="$b" '$1==b{print $4}' /tmp/dash_now.txt)
      pkn="${PICK_NAME[${pk:-0}]}"
      printf '<td class=cur>%s <small>%s/19 · %s</small></td>' "${step:-&mdash;}" "${dn:-0}" "$pkn"
    done
    echo '</tr>'
    # one ROW per step, a cell per bot
    idx=0
    for name in "${MS_NAMES[@]}"; do
      goalcls=""; [ "$idx" -eq "$GOAL_IDX" ] && goalcls=" goal"
      printf '<tr><td class="steplabel%s"><img class=ic src="%s/%s.png">%s</td>' "$goalcls" "$ICON_BASE" "${MS_ICON[$idx]}" "$name"
      lvl=$((idx + 1))
      for b in "${BOTS[@]}"; do
        # first_ms this bot recorded done=lvl (the milestone for completing step idx)
        fms=$(awk -v b="$b" -v l="$lvl" '$1==b && $2==l{print $3}' /tmp/dash_ms.txt)
        if [ -n "$fms" ]; then
          st=$(awk -F'|' -v b="$b" '$1==b{print $2}' /tmp/dash_start.txt)
          if [ -n "$st" ]; then rel=$(( (fms - st) / 1000 )); else rel=0; fi
          [ "$rel" -lt 0 ] && rel=0
          printf '<td class="ok%s">%d:%02d</td>' "$goalcls" "$((rel/60))" "$((rel%60))"
        else
          printf '<td class="no%s">&#10007;</td>' "$goalcls"
        fi
      done
      echo '</tr>'
      idx=$((idx+1))
    done
    echo '</table>'
    echo '<div class=sub style="margin-top:10px">cells show m:ss from each bot&#39;s start when the step completed, &#10007; not yet &bull; live from data/race.db &bull; refresh 4s</div>'
    echo '</body></html>'
  } > "$OUT.tmp" && mv -f "$OUT.tmp" "$OUT"
  sleep 4
done
