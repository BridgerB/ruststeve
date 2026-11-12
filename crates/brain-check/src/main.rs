//! The brain's integrity checker.
//!
//! ```text
//! cargo run -q -p brain-check                          the four invariants (exit 1 on any defect)
//! cargo run -q -p brain-check -- drift                 notes whose cited files changed since their pin
//! cargo run -q -p brain-check -- touches <source> <path>...   notes citing those paths
//! ```
//!
//! A sourceRef is `source:path#anchor`. `source` names a line of `brain/sources`; the anchor is a
//! symbol or the quoted text, matched after normalising markup and whitespace, and must match
//! exactly once. `git` sources are read with `git show <pin>:<path>` at the pin the note itself
//! records in `verifiedAgainst` (`source@sha`), so a note is checked against what was read.
//! Standard library only: the checker must build in seconds with no registry access.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// Notes that are hubs: never orphans, exempt from the per-note metadata rules.
const HUB_NAMES: [&str; 5] = [
    "operating-manual",
    "conventions",
    "rules",
    "glossary",
    "questions",
];

struct Source {
    pin: String,
    dir: Option<PathBuf>,
    worktree: bool,
}

struct Note {
    rel: String,
    path: PathBuf,
    meta: BTreeMap<String, Vec<String>>,
}

impl Note {
    fn first(&self, key: &str) -> Option<&str> {
        self.meta
            .get(key)
            .and_then(|v| v.first())
            .map(String::as_str)
    }
    fn all(&self, key: &str) -> &[String] {
        self.meta.get(key).map_or(&[], Vec::as_slice)
    }
    /// `name@sha` tokens of verifiedAgainst.
    fn pins(&self) -> BTreeMap<String, String> {
        self.all("verifiedAgainst")
            .iter()
            .flat_map(|v| v.split([' ', ',']))
            .filter_map(|t| t.split_once('@'))
            .map(|(n, p)| (n.to_string(), p.to_string()))
            .collect()
    }
    fn is_hub(&self) -> bool {
        let stem = stem(&self.rel);
        stem.starts_with('_') || HUB_NAMES.contains(&stem)
    }
}

fn stem(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(p),
    }
}

fn load_sources(brain: &Path, repo: &Path) -> BTreeMap<String, Source> {
    let text = fs::read_to_string(brain.join("sources")).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let cols: Vec<&str> = l.split_whitespace().collect();
            let [name, mode, pin, dir, rest @ ..] = cols.as_slice() else {
                return None;
            };
            let chosen = rest
                .first()
                .and_then(|env| std::env::var(env).ok())
                .unwrap_or_else(|| (*dir).to_string());
            let abs = if chosen == "." {
                repo.to_path_buf()
            } else {
                expand_home(&chosen)
            };
            let source = Source {
                pin: (*pin).to_string(),
                dir: abs.exists().then_some(abs),
                worktree: *mode == "worktree",
            };
            Some(((*name).to_string(), source))
        })
        .collect()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

/// A double-quoted YAML scalar: strip the quotes and undo `\"` and `\\`.
fn unquote(v: &str) -> String {
    let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return v.to_string();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match (c, chars.clone().next()) {
            ('\\', Some(n @ ('"' | '\\'))) => {
                out.push(n);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

fn parse_note(brain: &Path, path: PathBuf) -> Note {
    let text = fs::read_to_string(&path).unwrap_or_default();
    let rel = path
        .strip_prefix(brain)
        .unwrap_or(&path)
        .with_extension("")
        .to_string_lossy()
        .into_owned();
    let mut meta: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(front) = text
        .strip_prefix("---\n")
        .and_then(|t| t.split_once("\n---\n"))
        .map(|(f, _)| f)
    {
        let mut key = String::new();
        for line in front.lines() {
            let trimmed = line.trim_start();
            if line.starts_with(char::is_whitespace) && trimmed.starts_with("- ") && !key.is_empty()
            {
                meta.entry(key.clone())
                    .or_default()
                    .push(unquote(trimmed[2..].trim()));
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphabetic()) {
                continue;
            }
            key = k.to_string();
            let v = v.trim();
            if let Some(items) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                let list = items
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from);
                meta.insert(key.clone(), list.collect());
            } else if !v.is_empty() {
                meta.insert(key.clone(), vec![unquote(v)]);
            }
        }
    }
    Note { rel, path, meta }
}

/// What a line SAYS, not how it is written: a leading list marker, the presentation characters
/// `* _ \` #` and whitespace runs are dropped, so reformatting never breaks an anchor.
fn plain(s: &str) -> String {
    let t = s.trim_start();
    let digits = t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let after_marker = if t.starts_with(['-', '*', '+']) && t[1..].starts_with(char::is_whitespace)
    {
        &t[1..]
    } else if digits > 0
        && t[digits..].starts_with('.')
        && t[digits + 1..].starts_with(char::is_whitespace)
    {
        &t[digits + 1..]
    } else {
        t
    };
    let kept: String = after_marker
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#'))
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One string for the whole file, remembering each byte's line, so an anchor that a re-wrap
/// split across two lines still matches.
struct Flat {
    text: String,
    line_of: Vec<usize>,
}

fn flatten(text: &str) -> Flat {
    let mut flat = Flat {
        text: String::new(),
        line_of: Vec::new(),
    };
    for (i, raw) in text.lines().enumerate() {
        let s = plain(raw);
        if s.is_empty() {
            continue;
        }
        let chunk = format!(" {s}");
        flat.line_of.extend(std::iter::repeat_n(i + 1, chunk.len()));
        flat.text.push_str(&chunk);
    }
    flat
}

fn occurrences(hay: &str, needle: &str) -> Vec<usize> {
    let mut at = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(needle) {
        at.push(from + i);
        from += i + hay[from + i..].chars().next().map_or(1, char::len_utf8);
    }
    at
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

enum Resolved {
    Line,
    Unchecked,
    Error(String),
}

struct Resolver<'a> {
    sources: &'a BTreeMap<String, Source>,
    flats: BTreeMap<String, Option<Flat>>,
}

impl Resolver<'_> {
    fn flat(&mut self, name: &str, pin: &str, path: &str) -> Option<&Flat> {
        let key = format!("{name}@{pin}:{path}");
        if !self.flats.contains_key(&key) {
            let source = &self.sources[name];
            let dir = source.dir.as_deref()?;
            let text = if source.worktree {
                fs::read_to_string(dir.join(path)).ok()
            } else {
                git(dir, &["show", &format!("{pin}:{path}")])
            };
            self.flats.insert(key.clone(), text.as_deref().map(flatten));
        }
        self.flats.get(&key).and_then(Option::as_ref)
    }

    fn resolve(&mut self, note: &Note, r: &str) -> Resolved {
        let (Some(colon), Some(hash)) = (r.find(':'), r.find('#')) else {
            return Resolved::Error("not source:path#anchor".into());
        };
        if hash < colon {
            return Resolved::Error("not source:path#anchor".into());
        }
        let (name, path, anchor) = (&r[..colon], &r[colon + 1..hash], plain(&r[hash + 1..]));
        let Some(source) = self.sources.get(name) else {
            return Resolved::Error(format!("unknown source \"{name}\""));
        };
        if anchor.is_empty() {
            return Resolved::Error("no anchor: a bare path cites nothing".into());
        }
        if source.dir.is_none() {
            return Resolved::Unchecked;
        }
        let pin = if source.worktree {
            Some("worktree".to_string())
        } else {
            note.pins().get(name).cloned()
        };
        let Some(pin) = pin else {
            return Resolved::Error(format!("no {name}@<sha> in verifiedAgainst"));
        };
        let Some(flat) = self.flat(name, &pin, path) else {
            return Resolved::Error(format!("no such file at {pin}"));
        };
        match occurrences(&flat.text, &anchor).as_slice() {
            [] => Resolved::Error("matches nothing".into()),
            [_] => Resolved::Line,
            many => {
                let lines: Vec<String> =
                    many.iter().map(|&i| flat.line_of[i].to_string()).collect();
                Resolved::Error(format!(
                    "matches {} places (lines {})",
                    many.len(),
                    lines.join(", ")
                ))
            }
        }
    }
}

/// `[[target|alias]]` targets, with fenced and inline code removed first. The frontmatter's
/// sourceRefs are skipped too: an anchor quoting source text (a Clojure spec opens with `[[`) is
/// a citation, not a link.
fn links(text: &str) -> Vec<String> {
    let mut prose = String::new();
    let mut fenced = false;
    let mut in_front = text.starts_with("---\n");
    let mut in_refs = false;
    for (i, line) in text.lines().enumerate() {
        if in_front {
            if i > 0 && line == "---" {
                in_front = false;
                continue;
            }
            if !line.starts_with(char::is_whitespace) {
                in_refs = line.starts_with("sourceRefs:");
            }
            if in_refs {
                continue;
            }
        }
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            let mut inline = false;
            prose.extend(line.chars().filter(|&c| {
                if c == '`' {
                    inline = !inline;
                }
                !inline && c != '`'
            }));
            prose.push('\n');
        }
    }
    let mut out = Vec::new();
    let mut rest = prose.as_str();
    while let Some(start) = rest.find("[[") {
        let Some(len) = rest[start + 2..].find("]]") else {
            break;
        };
        let inner = rest[start + 2..start + 2 + len].replace("\\|", "|");
        let target = inner
            .split('|')
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("")
            .trim();
        if !target.is_empty() {
            out.push(target.to_string());
        }
        rest = &rest[start + 2 + len + 2..];
    }
    out
}

fn ref_parts(r: &str) -> Option<(&str, &str)> {
    let colon = r.find(':')?;
    let hash = r.find('#')?;
    (colon < hash).then(|| (&r[..colon], &r[colon + 1..hash]))
}

fn report(label: &str, items: &[String]) {
    println!("{label}: {}", items.len());
    for i in items {
        println!("  {i}");
    }
}

fn main() -> ExitCode {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repo = repo.canonicalize().unwrap_or(repo);
    let brain = repo.join("brain");
    let sources = load_sources(&brain, &repo);
    let mut files = Vec::new();
    walk(&brain, &mut files);
    let notes: Vec<Note> = files.into_iter().map(|p| parse_note(&brain, p)).collect();
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("touches") => {
            let name = args.get(1).map_or("", String::as_str);
            for n in &notes {
                let hit = n
                    .all("sourceRefs")
                    .iter()
                    .filter_map(|r| ref_parts(r))
                    .any(|(s, p)| s == name && args[2..].iter().any(|a| a == p));
                if hit {
                    println!("{}", n.rel);
                }
            }
            ExitCode::SUCCESS
        }
        Some("drift") => {
            let mut stale = 0;
            for n in &notes {
                let pins = n.pins();
                let mut by_source: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
                for (s, p) in n.all("sourceRefs").iter().filter_map(|r| ref_parts(r)) {
                    by_source.entry(s).or_default().insert(p);
                }
                for (name, paths) in by_source {
                    let (Some(source), Some(pin)) = (sources.get(name), pins.get(name)) else {
                        continue;
                    };
                    let Some(dir) = source.dir.as_deref() else {
                        continue;
                    };
                    let head = if source.worktree {
                        "HEAD"
                    } else {
                        source.pin.as_str()
                    };
                    let range = format!("{pin}..{head}");
                    let mut cmd = vec!["diff", "--name-only", range.as_str(), "--"];
                    cmd.extend(paths.iter().copied());
                    let changed =
                        git(dir, &cmd).unwrap_or_else(|| format!("(cannot diff {range})"));
                    if !changed.trim().is_empty() {
                        stale += 1;
                        println!("{}  [{name} {range}]", n.rel);
                        for line in changed.lines() {
                            println!("  {line}");
                        }
                    }
                }
            }
            println!("stale notes: {stale}");
            ExitCode::SUCCESS
        }
        _ => check(&notes, &sources),
    }
}

fn check(notes: &[Note], sources: &BTreeMap<String, Source>) -> ExitCode {
    let by_path: BTreeSet<&str> = notes.iter().map(|n| n.rel.as_str()).collect();
    let mut by_stem: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for n in notes {
        by_stem.entry(stem(&n.rel)).or_default().push(&n.rel);
    }
    let mut inbound: BTreeMap<&str, usize> = notes.iter().map(|n| (n.rel.as_str(), 0)).collect();
    let (mut broken, mut ambiguous, mut meta, mut unresolved) = (vec![], vec![], vec![], vec![]);
    let (mut resolved_refs, mut unchecked) = (0, 0);
    let mut resolver = Resolver {
        sources,
        flats: BTreeMap::new(),
    };

    for n in notes {
        let text = fs::read_to_string(&n.path).unwrap_or_default();
        for target in links(&text) {
            let hit: Option<&str> = if by_path.contains(target.as_str()) {
                by_path.get(target.as_str()).copied()
            } else {
                match by_stem.get(stem(&target)).map(Vec::as_slice) {
                    Some([one]) => Some(*one),
                    Some(many) if many.len() > 1 => {
                        ambiguous.push(format!("{} -> {target} ({})", n.rel, many.join(", ")));
                        None
                    }
                    _ => {
                        broken.push(format!("{} -> {target}", n.rel));
                        None
                    }
                }
            };
            if let Some(t) = hit.filter(|t| *t != n.rel) {
                *inbound.entry(t).or_default() += 1;
            }
        }

        for key in ["title", "type"] {
            if n.first(key).is_none() {
                meta.push(format!("{}: no {key}", n.rel));
            }
        }
        let refs = n.all("sourceRefs");
        if !n.is_hub() {
            let verified = n.first("status") == Some("verified");
            if n.first("description").is_none() {
                meta.push(format!("{}: no description", n.rel));
            }
            if verified && refs.is_empty() {
                meta.push(format!("{}: verified with no sourceRefs", n.rel));
            }
            if verified && n.first("verifiedAgainst").is_none() {
                meta.push(format!("{}: verified with no verifiedAgainst", n.rel));
            }
            if n.all("related").len() < 2 {
                meta.push(format!("{}: fewer than two related links", n.rel));
            }
        }
        for r in refs {
            match resolver.resolve(n, r) {
                Resolved::Line => resolved_refs += 1,
                Resolved::Unchecked => unchecked += 1,
                Resolved::Error(why) => unresolved.push(format!("{}: {r}  -- {why}", n.rel)),
            }
        }
    }

    let orphans: Vec<String> = notes
        .iter()
        .filter(|n| !n.is_hub() && inbound.get(n.rel.as_str()) == Some(&0))
        .map(|n| n.rel.clone())
        .collect();
    println!(
        "notes: {}, sourceRefs resolved: {resolved_refs}, unchecked (checkout missing): {unchecked}",
        notes.len()
    );
    report("broken links", &broken);
    report("ambiguous links", &ambiguous);
    report("orphans", &orphans);
    report("unresolved sourceRefs", &unresolved);
    report("metadata", &meta);
    let clean = [&broken, &ambiguous, &orphans, &unresolved, &meta]
        .iter()
        .all(|v| v.is_empty());
    if clean {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
