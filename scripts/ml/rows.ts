// The attempt event log reader (cycle 5, decision 5). Rows live in per-process files under
// data/attempts/ (one writer each, no interleaving) plus the cycle-4 single file data/attempts.jsonl,
// which is read-only now. Every reader goes through here.
//
//   readRows()                     → all rows from data/attempts/*.jsonl + data/attempts.jsonl
//   readRows("some/file.jsonl")    → one explicit file (or a directory of .jsonl files)
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

export const ATTEMPTS_DIR = "data/attempts";
export const LEGACY_FILE = "data/attempts.jsonl";

/** Unparseable lines seen by the last readRows call (interleaved cycle-4 lines, a partial last line). */
export let unparseable = 0;

const files = (path?: string): string[] => {
  if (path) {
    if (!existsSync(path)) return [];
    return statSync(path).isDirectory() ? readdirSync(path).filter((f) => f.endsWith(".jsonl")).map((f) => join(path, f)) : [path];
  }
  const dir = existsSync(ATTEMPTS_DIR) ? readdirSync(ATTEMPTS_DIR).filter((f) => f.endsWith(".jsonl")).map((f) => join(ATTEMPTS_DIR, f)) : [];
  return [...(existsSync(LEGACY_FILE) ? [LEGACY_FILE] : []), ...dir.sort()];
};

export const readRows = <T = Record<string, unknown>>(path?: string): T[] => {
  unparseable = 0;
  const out: T[] = [];
  for (const f of files(path)) {
    for (const line of readFileSync(f, "utf8").split("\n")) {
      if (!line.trim()) continue;
      try {
        out.push(JSON.parse(line) as T);
      } catch {
        unparseable++;
      }
    }
  }
  return out;
};
