#!/usr/bin/env bash
# Live race dashboard → /tmp/race.html (auto-refreshing). Regenerates from the
# race-*.log files every few seconds.
DIR=/Users/bridger/Developer/mc/upstream/ruststeve
OUT=/tmp/race.html

# The FULL speedrun roadmap (spawn → Ender Dragon). Steps past "Enter Nether" are
# not yet implemented in ruststeve, so their patterns never match and they stay ✗ —
# they're shown so the dashboard reflects the whole plan, not just what's coded.
# GOAL_IDX marks the current race finish line (Enter Nether).
MS_NAMES=("Logs" "Planks" "Table" "Sticks" "Wood Pick" "Cobble" "Stone Pick" "Furnace" "Coal" "Iron Ore" "Smelt" "IRON PICK" \
  "Buckets" "Water Buckets" "Gather Food" "Flint & Steel" "Build Portal" "ENTER NETHER" \
  "Nether Fortress" "Kill Blazes" "Hunt Endermen" "Return Overworld" "Eyes of Ender" "Find Stronghold" \
  "Activate End Portal" "Bow & Arrows" "Enter The End" "End Crystals" "KILL DRAGON")
GOAL_IDX=17   # "ENTER NETHER" — the current race goal row (highlighted)
MS_PAT=(
  'wood: [1-9]|gathered [1-9]'
  'crafted planks from logs|crafted [a-z_]*planks \(have'
  'table: placed|table: dug a niche|confirmed at|reusing remembered table|crafted [a-z_]*crafting_table'
  'crafted (1x )?stick'
  'crafted (1x )?wooden_pickaxe'
  'mined [1-9][0-9]*/[0-9]+ cobblestone|stone: [1-9]'
  'crafted (1x )?stone_pickaxe'
  'crafted (1x )?furnace'
  'mined [1-9][0-9]*/[0-9]+ coal|ore: [1-9][0-9]* coal'
  'mined [1-9][0-9]*/[0-9]+ iron|ore: [1-9][0-9]* iron'
  'smelted [1-9]'
  'crafted (1x )?iron_pickaxe'
  'crafted (1x )?bucket\b'
  'filled [1-9][0-9]*/[0-9]+ water bucket|bucket: [1-9][0-9]* water'
  'gathered [1-9][0-9]* food|cooked [1-9]'
  'crafted (1x )?flint_and_steel'
  'nether portal cast & lit|portal_lit|frame pass: 10/10'
  'entered the nether|RACE GOAL REACHED'
  '__nyi_nether_fortress__'
  '__nyi_blazes__'
  '__nyi_endermen__'
  '__nyi_return_overworld__'
  '__nyi_eyes_of_ender__'
  '__nyi_stronghold__'
  '__nyi_end_portal__'
  '__nyi_bow__'
  '__nyi_enter_end__'
  '__nyi_end_crystals__'
  '__nyi_dragon__'
)
# Minecraft texture icons (one per step, matching MS_NAMES order).
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

STATE=/tmp/race-ms-times.tsv   # remembers race-time each (bot,milestone) was first reached
while true; do
  t=$(grep -oE 't=[0-9]+s' "$DIR/race-orchestrator.log" 2>/dev/null | tail -1)
  winner=$(grep -iE 'WINNER|RACE OVER' "$DIR/race-orchestrator.log" 2>/dev/null | tail -1)
  alive=$(pgrep -f 'target/release/ruststeve' | wc -l | tr -d ' ')
  now=$(date +%s)
  # EXACT race seconds from wall-clock (not the orchestrator's coarse 20s t=).
  rs=$(cat /tmp/race-start 2>/dev/null); rs=${rs:-$now}
  T=$(( now - rs )); [ "$T" -lt 0 ] && T=0
  tstr=$(printf '%d:%02d' $((T/60)) $((T%60)))
  # reset remembered milestone times when a NEW race starts (clock jumps back)
  lastT=$(cat /tmp/race-ms-lastT 2>/dev/null || echo 0)
  [ "$T" -lt "$lastT" ] && : > "$STATE"
  echo "$T" > /tmp/race-ms-lastT
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
    echo "<div class=sub>elapsed ${tstr} &nbsp;&bull;&nbsp; alive ${alive}/5 &nbsp;&bull;&nbsp; <span class=win>${winner}</span></div>"
    # --- per-bot header info (current step / pick / idle) ---
    for i in $(seq 0 4); do
      log="$DIR/race-$i.log"
      line=$(grep -E '^\[minecraft:' "$log" 2>/dev/null | tail -1)
      B_STEP[$i]=$(echo "$line" | grep -oE '→ [A-Za-z ]+\(' | sed 's/[→(]//g; s/ *$//')
      B_PICK[$i]=$(echo "$line" | grep -oE 'pick=[A-Za-z]+' | sed 's/pick=//')
      B_HCLS[$i]=botcol
      if [ -f "$log" ]; then
        age=$(( now - $(stat -f %m "$log" 2>/dev/null || echo 0) ))
        [ "$age" -gt 30 ] && B_HCLS[$i]="botcol idle"
      fi
    done

    echo '<table>'
    # bots across the TOP
    echo '<tr><th class=steplabel>Step</th>'
    for i in $(seq 0 4); do
      idletag=""; case "${B_HCLS[$i]}" in *idle*) idletag=" <small>(idle)</small>";; esac
      printf '<th class="%s">race-%02d<br><small>z=%d</small>%s</th>' "${B_HCLS[$i]}" "$i" "$((300+80*i))" "$idletag"
    done
    echo '</tr>'
    # current step per bot
    echo '<tr><td class=steplabel>Now doing</td>'
    for i in $(seq 0 4); do
      printf '<td class=cur>%s <small>%s</small></td>' "${B_STEP[$i]:-&mdash;}" "${B_PICK[$i]:+(${B_PICK[$i]})}"
    done
    echo '</tr>'
    # one ROW per step (full name on the LEFT), a cell per bot
    idx=0
    for p in "${MS_PAT[@]}"; do
      goalcls=""; [ "$idx" -eq "$GOAL_IDX" ] && goalcls=" goal"
      printf '<tr><td class="steplabel%s"><img class=ic src="%s/%s.png">%s</td>' "$goalcls" "$ICON_BASE" "${MS_ICON[$idx]}" "${MS_NAMES[$idx]}"
      for i in $(seq 0 4); do
        log="$DIR/race-$i.log"
        if grep -qE "$p" "$log" 2>/dev/null; then
          rt=$(awk -v i="$i" -v m="$idx" '$1==i && $2==m {print $3; exit}' "$STATE" 2>/dev/null)
          if [ -z "$rt" ]; then rt=$T; printf '%s %s %s\n' "$i" "$idx" "$T" >> "$STATE"; fi
          printf '<td class="ok%s">%d:%02d</td>' "$goalcls" "$((rt/60))" "$((rt%60))"
        else
          printf '<td class="no%s">&#10007;</td>' "$goalcls"
        fi
      done
      echo '</tr>'
      idx=$((idx+1))
    done
    echo '</table>'
    echo '<div class=sub style="margin-top:10px">cells show race-time (m:ss) when reached, &#10007; not yet &bull; auto-refreshes every 4s</div>'
    echo '</body></html>'
  } > "$OUT.tmp" && mv -f "$OUT.tmp" "$OUT"   # atomic swap so the browser never reads a half-written file
  sleep 4
done
