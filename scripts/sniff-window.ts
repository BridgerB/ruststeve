// Print a SNIFF_JSON capture's window traffic compactly: one line per container packet, items as
// slot:count×itemId, so a craft exchange can be read (and cut into a test fixture).
//   node scripts/sniff-window.ts data/sniff-rust-gym-008.db [from_id] [to_id]
import { DatabaseSync } from "node:sqlite";

const [db = "", from = "0", to = "1e12"] = process.argv.slice(2);
const rows = new DatabaseSync(db, { readOnly: true })
  .prepare("SELECT id, ts_ms, dir, name, summary FROM packets WHERE id BETWEEN ? AND ? ORDER BY id")
  .all(Number(from), Number(to)) as { id: number; ts_ms: number; dir: string; name: string; summary: string }[];
type Slot = { itemCount?: number; itemId?: number } | null;
const slot = (s: Slot) => (s && s.itemCount ? `${s.itemCount}×${s.itemId}` : "");
for (const r of rows) {
  let p: Record<string, unknown> = {};
  try {
    p = JSON.parse(r.summary);
  } catch {
    p = { raw: r.summary };
  }
  let body = "";
  if (r.name === "container_set_content") {
    const items = (p.items as Slot[]) ?? [];
    body = `win=${p.windowId} state=${p.stateId} ` + items.map((s, i) => (slot(s) ? `${i}:${slot(s)}` : "")).filter(Boolean).join(" ") + ` cursor=${slot(p.carriedItem as Slot)}`;
  } else if (r.name === "container_set_slot") {
    body = `win=${p.windowId} state=${p.stateId} slot ${p.slot} = ${slot(p.item as Slot) || "empty"}`;
  } else if (r.name === "container_click") {
    const changed = ((p.changedSlots as { location: number; item: Slot }[]) ?? []).map((c) => `${c.location}:${slot(c.item) || "empty"}`).join(" ");
    body = `win=${p.windowId} state=${p.stateId} slot=${p.slot} btn=${p.mouseButton} mode=${p.mode} changed[${changed}] cursor=${slot(p.cursorItem as Slot)}`;
  } else {
    body = JSON.stringify(p).slice(0, 200);
  }
  console.log(`${r.id}\t${r.ts_ms}\t${r.dir}\t${r.name}\t${body}`);
}
