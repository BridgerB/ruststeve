// Cut a window-packet fixture from a SNIFF_JSON capture (decision 7's unit test replays it).
//   node scripts/sniff-fixture.ts data/sniff-rust-gym-008.db <from_id> <to_id> > tests/fixtures/<name>.json
// Keeps the server→bot container_set_content / container_set_slot packets, with each slot reduced to
// [itemId, count] (null when empty).
import { DatabaseSync } from "node:sqlite";

const [db = "", from = "0", to = "1e12"] = process.argv.slice(2);
const rows = new DatabaseSync(db, { readOnly: true })
  .prepare("SELECT id, name, summary FROM packets WHERE dir = 'in' AND name IN ('container_set_content','container_set_slot') AND id BETWEEN ? AND ? ORDER BY id")
  .all(Number(from), Number(to)) as { id: number; name: string; summary: string }[];
type Slot = { itemCount?: number; itemId?: number } | null;
const s = (x: Slot) => (x && x.itemCount ? [x.itemId, x.itemCount] : null);
const out = rows.map((r) => {
  const p = JSON.parse(r.summary);
  return r.name === "container_set_content"
    ? { id: r.id, name: r.name, windowId: p.windowId, stateId: p.stateId, items: (p.items as Slot[]).map(s), carried: s(p.carriedItem) }
    : { id: r.id, name: r.name, windowId: p.windowId, stateId: p.stateId, slot: p.slot, item: s(p.item) };
});
console.log(JSON.stringify(out));
