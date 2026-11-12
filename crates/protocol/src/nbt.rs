//! Network NBT (brain/wire/wire-nbt-text.md): a text component in 26.x is an anonymous root tag,
//! a type byte then its payload (no root name since 1.20.2). Read into a plain value; the
//! component helpers below read a death message the way clojurecraft's `text.clj` does: a
//! translate key reads as itself with its arguments' text after it, no English table.

use crate::ParseError;
use crate::bytes::Reader;

/// How deep tags may nest; deeper is a malformed frame, never a stack overflow.
const MAX_DEPTH: usize = 64;

/// One NBT value.
#[derive(Clone, Debug, PartialEq)]
pub enum Nbt {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Nbt>),
    /// Entries in the order sent.
    Compound(Vec<(String, Nbt)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

/// The anonymous root tag at the reader (a text component, a kick reason). Tag 0 (end) as the
/// root is an empty compound.
pub fn read_root(r: &mut Reader<'_>) -> Result<Nbt, ParseError> {
    let tag = r.u8()?;
    if tag == 0 {
        return Ok(Nbt::Compound(Vec::new()));
    }
    payload(r, tag, 0)
}

fn bad(what: &'static str) -> ParseError {
    ParseError::BadNbt(what)
}

/// NBT strings: a u16 length then modified UTF-8, read leniently (a lone surrogate is replaced).
fn nbt_string(r: &mut Reader<'_>) -> Result<String, ParseError> {
    let n = usize::from(r.u16()?);
    Ok(String::from_utf8_lossy(r.take(n)?).into_owned())
}

/// An array length: a signed i32, never negative.
fn count(r: &mut Reader<'_>) -> Result<usize, ParseError> {
    let n = r.i32()?;
    usize::try_from(n).map_err(|_| ParseError::NegativeLength(i64::from(n)))
}

/// Reads `n` elements, refusing a count the remaining bytes cannot hold (`min` bytes each).
fn many<T>(
    r: &mut Reader<'_>,
    n: usize,
    min: usize,
    mut one: impl FnMut(&mut Reader<'_>) -> Result<T, ParseError>,
) -> Result<Vec<T>, ParseError> {
    if n.saturating_mul(min) > r.remaining() {
        return Err(ParseError::Eof {
            at: r.consumed(),
            need: n.saturating_mul(min),
        });
    }
    (0..n).map(|_| one(r)).collect()
}

fn payload(r: &mut Reader<'_>, tag: u8, depth: usize) -> Result<Nbt, ParseError> {
    if depth > MAX_DEPTH {
        return Err(bad("nested too deep"));
    }
    Ok(match tag {
        1 => Nbt::Byte(r.i8()?),
        2 => Nbt::Short(r.i16()?),
        3 => Nbt::Int(r.i32()?),
        4 => Nbt::Long(r.i64()?),
        5 => Nbt::Float(r.f32()?),
        6 => Nbt::Double(r.f64()?),
        7 => {
            let n = count(r)?;
            Nbt::ByteArray(many(r, n, 1, |r| r.i8())?)
        }
        8 => Nbt::String(nbt_string(r)?),
        9 => {
            let inner = r.u8()?;
            let n = count(r)?;
            if inner == 0 {
                // An empty list is typed end; a non-empty one of end tags is malformed.
                if n > 0 {
                    return Err(bad("a list of end tags"));
                }
                Nbt::List(Vec::new())
            } else {
                Nbt::List(many(r, n, 1, |r| payload(r, inner, depth + 1))?)
            }
        }
        10 => {
            let mut entries = Vec::new();
            loop {
                let t = r.u8()?;
                if t == 0 {
                    break;
                }
                let name = nbt_string(r)?;
                entries.push((name, payload(r, t, depth + 1)?));
            }
            Nbt::Compound(entries)
        }
        11 => {
            let n = count(r)?;
            Nbt::IntArray(many(r, n, 4, |r| r.i32())?)
        }
        12 => {
            let n = count(r)?;
            Nbt::LongArray(many(r, n, 8, |r| r.i64())?)
        }
        _ => return Err(bad("an unknown tag type")),
    })
}

impl Nbt {
    /// A compound's entry by name.
    pub fn get(&self, key: &str) -> Option<&Nbt> {
        match self {
            Nbt::Compound(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Nbt::String(s) => Some(s),
            _ => None,
        }
    }

    fn parts(&self) -> &[Nbt] {
        match self {
            Nbt::List(v) => v,
            _ => &[],
        }
    }
}

/// A component as one line: a string as it is; a number as its text; a list as its parts
/// joined; a compound as its "text" or, for a "translate" key, the key then each "with"
/// argument's text after a space, then "extra". A compound whose only key is "" is a list
/// element NBT had to wrap. Total: a malformed component reads as what it holds.
pub fn plain(c: &Nbt) -> String {
    match c {
        Nbt::String(s) => s.clone(),
        Nbt::Byte(v) => v.to_string(),
        Nbt::Short(v) => v.to_string(),
        Nbt::Int(v) => v.to_string(),
        Nbt::Long(v) => v.to_string(),
        Nbt::Float(v) => v.to_string(),
        Nbt::Double(v) => v.to_string(),
        Nbt::List(v) => v.iter().map(plain).collect(),
        Nbt::Compound(entries) => {
            if let [(k, v)] = entries.as_slice()
                && k.is_empty()
            {
                return plain(v);
            }
            let head = match c.get("translate").and_then(Nbt::as_str) {
                Some(key) => {
                    let args = c.get("with").map_or(&[][..], Nbt::parts);
                    std::iter::once(key.to_owned())
                        .chain(args.iter().map(plain))
                        .collect::<Vec<_>>()
                        .join(" ")
                }
                None => c.get("text").map(plain).unwrap_or_default(),
            };
            let extra: String = c
                .get("extra")
                .map_or(&[][..], Nbt::parts)
                .iter()
                .map(plain)
                .collect();
            head + &extra
        }
        Nbt::ByteArray(_) | Nbt::IntArray(_) | Nbt::LongArray(_) => String::new(),
    }
}

/// The server's account of a death: its translate key (`death.attack.mob`), the victim and the
/// killer as the component names them (an entity's translate key loses its
/// `entity.minecraft.` prefix: `zombie`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Death {
    /// The whole message as `plain` reads it.
    pub text: String,
    /// `None` when the component is not a death message (its key does not begin `death.`).
    pub key: Option<String>,
    pub victim: Option<String>,
    pub by: Option<String>,
}

pub fn death(c: &Nbt) -> Death {
    let key = c
        .get("translate")
        .and_then(Nbt::as_str)
        .filter(|k| k.starts_with("death."))
        .map(str::to_owned);
    let args: Vec<String> = c
        .get("with")
        .map_or(&[][..], Nbt::parts)
        .iter()
        .map(plain)
        .collect();
    let named = |s: &String| {
        s.strip_prefix("entity.minecraft.")
            .map_or_else(|| s.clone(), str::to_owned)
    };
    Death {
        text: plain(c),
        victim: key.as_ref().and(args.first().cloned()),
        by: key.as_ref().and(args.get(1).map(named)),
        key,
    }
}

/// The anonymous root tag as network NBT: the inverse of `read_root` (the server model's text).
pub fn write_root(v: &Nbt) -> Vec<u8> {
    let mut out = vec![tag_of(v)];
    write_payload(&mut out, v);
    out
}

fn tag_of(v: &Nbt) -> u8 {
    match v {
        Nbt::Byte(_) => 1,
        Nbt::Short(_) => 2,
        Nbt::Int(_) => 3,
        Nbt::Long(_) => 4,
        Nbt::Float(_) => 5,
        Nbt::Double(_) => 6,
        Nbt::ByteArray(_) => 7,
        Nbt::String(_) => 8,
        Nbt::List(_) => 9,
        Nbt::Compound(_) => 10,
        Nbt::IntArray(_) => 11,
        Nbt::LongArray(_) => 12,
    }
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    let n = u16::try_from(s.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(s.as_bytes().get(..usize::from(n)).unwrap_or_default());
}

fn put_len(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&i32::try_from(n).unwrap_or(i32::MAX).to_be_bytes());
}

fn write_payload(out: &mut Vec<u8>, v: &Nbt) {
    match v {
        Nbt::Byte(b) => out.extend_from_slice(&b.to_be_bytes()),
        Nbt::Short(s) => out.extend_from_slice(&s.to_be_bytes()),
        Nbt::Int(i) => out.extend_from_slice(&i.to_be_bytes()),
        Nbt::Long(l) => out.extend_from_slice(&l.to_be_bytes()),
        Nbt::Float(f) => out.extend_from_slice(&f.to_be_bytes()),
        Nbt::Double(d) => out.extend_from_slice(&d.to_be_bytes()),
        Nbt::ByteArray(a) => {
            put_len(out, a.len());
            out.extend(a.iter().flat_map(|b| b.to_be_bytes()));
        }
        Nbt::String(s) => put_string(out, s),
        Nbt::List(items) => {
            out.push(items.first().map_or(0, tag_of));
            put_len(out, items.len());
            for i in items {
                write_payload(out, i);
            }
        }
        Nbt::Compound(entries) => {
            for (k, v) in entries {
                out.push(tag_of(v));
                put_string(out, k);
                write_payload(out, v);
            }
            out.push(0);
        }
        Nbt::IntArray(a) => {
            put_len(out, a.len());
            out.extend(a.iter().flat_map(|i| i.to_be_bytes()));
        }
        Nbt::LongArray(a) => {
            put_len(out, a.len());
            out.extend(a.iter().flat_map(|l| l.to_be_bytes()));
        }
    }
}

/// The component a server sends for a death: `key` with the victim's name and, when a mob did
/// it, the mob's entity name, as the `with` arguments (clojurecraft's `text/death-component`).
pub fn death_component(key: &str, victim: &str, by: Option<&str>) -> Nbt {
    let mut with = vec![Nbt::Compound(vec![(
        "text".to_owned(),
        Nbt::String(victim.to_owned()),
    )])];
    if let Some(by) = by {
        with.push(Nbt::Compound(vec![(
            "translate".to_owned(),
            Nbt::String(format!("entity.minecraft.{by}")),
        )]));
    }
    Nbt::Compound(vec![
        ("translate".to_owned(), Nbt::String(key.to_owned())),
        ("with".to_owned(), Nbt::List(with)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a component the way the server does, for the tests.
    fn str_tag(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&u16::try_from(s.len()).unwrap_or(0).to_be_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    fn death_message() -> Vec<u8> {
        // {translate: "death.attack.mob", with: [{text: "rs-003"}, {translate: "entity.minecraft.zombie"}]}
        let mut b = vec![10];
        b.push(8);
        str_tag(&mut b, "translate");
        str_tag(&mut b, "death.attack.mob");
        b.push(9);
        str_tag(&mut b, "with");
        b.push(10);
        b.extend_from_slice(&2i32.to_be_bytes());
        b.push(8);
        str_tag(&mut b, "text");
        str_tag(&mut b, "rs-003");
        b.push(0);
        b.push(8);
        str_tag(&mut b, "translate");
        str_tag(&mut b, "entity.minecraft.zombie");
        b.push(0);
        b.push(0);
        b
    }

    #[test]
    fn a_death_reads_as_the_server_says_it() {
        let bytes = death_message();
        let c = read_root(&mut Reader::new(&bytes));
        assert!(c.is_ok(), "{c:?}");
        let d = death(&c.unwrap_or(Nbt::Compound(Vec::new())));
        assert_eq!(d.key.as_deref(), Some("death.attack.mob"));
        assert_eq!(d.victim.as_deref(), Some("rs-003"));
        assert_eq!(d.by.as_deref(), Some("zombie"));
        assert_eq!(d.text, "death.attack.mob rs-003 entity.minecraft.zombie");
    }

    #[test]
    fn a_plain_string_root() {
        let mut b = vec![8];
        str_tag(&mut b, "hello");
        let c = read_root(&mut Reader::new(&b));
        assert_eq!(c, Ok(Nbt::String("hello".to_owned())));
        assert_eq!(death(&Nbt::String("hello".to_owned())).key, None);
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_crash() {
        let mut b = Vec::new();
        for _ in 0..1000 {
            b.extend_from_slice(&[9, 9]);
            b.extend_from_slice(&1i32.to_be_bytes());
        }
        assert!(read_root(&mut Reader::new(&b)).is_err());
    }

    #[test]
    fn a_huge_array_count_is_an_error_not_an_allocation() {
        let mut b = vec![11];
        b.extend_from_slice(&i32::MAX.to_be_bytes());
        assert!(read_root(&mut Reader::new(&b)).is_err());
    }
}
