//! Server truth (issue 16e, phase D): what the race's own server holds about the bot and the
//! entities near it, asked over RCON by the shell once a minute and at the end, and set against
//! what the bot believed. Truth is for a reader only: no core crate depends on this module, so
//! nothing here can reach the planner (strict no X-ray).

use serde::{Deserialize, Serialize};

/// The RCON command for the entities within 32 blocks of `bot` (nearest 12, no items, orbs or
/// players: clojurecraft's `near-commands`).
pub fn near_command(bot: &str) -> String {
    format!(
        "execute at {bot} as @e[distance=..32,limit=12,sort=nearest,type=!player,type=!item,type=!experience_orb] run data get entity @s"
    )
}

/// The RCON command for the bot itself.
pub fn self_command(bot: &str) -> String {
    format!("data get entity {bot}")
}

/// One entity as the server holds it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TruthEntity {
    /// `blaze`, without the namespace.
    pub kind: String,
    pub pos: Option<[f64; 3]>,
    pub health: Option<f64>,
    /// An enderman's or a piglin's anger, in ticks left.
    pub anger: Option<i64>,
    /// A creeper's fuse, ticks.
    pub fuse: Option<i64>,
}

/// One sample: the server tick the bot was at, the raw replies, and the entities read from them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TruthSample {
    pub at_tick: u64,
    pub entities: Vec<TruthEntity>,
    pub bot: Option<TruthEntity>,
    /// The bot's held items with their damage as the server holds them (`id damage`).
    pub bot_items: Vec<String>,
    pub raw: String,
}

/// A run's truth file (`<bot>.truth.rst`): the samples of the last minutes, oldest first, and
/// the blind spots found at the end.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TruthFile {
    pub format_version: u32,
    pub run: String,
    pub bot: String,
    pub samples: Vec<TruthSample>,
    pub blind: Vec<String>,
}

/// The text after `key` up to the next `,` `}` or `]` at the same depth, trimmed of quotes and an
/// SNBT type suffix.
fn value_after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let at = s.find(key)? + key.len();
    let rest = &s[at..];
    let end = rest.find([',', '}', ']']).unwrap_or(rest.len());
    Some(rest[..end].trim().trim_matches('"'))
}

fn number(v: &str) -> Option<f64> {
    v.trim()
        .trim_end_matches(['d', 'f', 's', 'b', 'L'])
        .parse()
        .ok()
}

/// One entity's SNBT as `data get entity` prints it.
pub fn parse_entity(snbt: &str) -> TruthEntity {
    let kind = value_after(snbt, "id: ")
        .map(|k| k.trim_start_matches("minecraft:").to_owned())
        .unwrap_or_default();
    let pos = snbt.find("Pos: [").and_then(|i| {
        let rest = &snbt[i + 6..];
        let end = rest.find(']')?;
        let v: Vec<f64> = rest[..end].split(',').filter_map(number).collect();
        (v.len() == 3).then(|| [v[0], v[1], v[2]])
    });
    TruthEntity {
        kind,
        pos,
        health: value_after(snbt, "Health: ").and_then(number),
        anger: value_after(snbt, "AngerTime: ")
            .or_else(|| value_after(snbt, "anger_end_time: "))
            .and_then(number)
            .map(|v| v as i64),
        fuse: value_after(snbt, "Fuse: ")
            .and_then(number)
            .map(|v| v as i64),
    }
}

/// Every entity in a reply: RCON joins the messages, each `<name> has the following entity
/// data: {...}`.
pub fn parse_reply(reply: &str) -> Vec<TruthEntity> {
    reply
        .split("has the following entity data: ")
        .skip(1)
        .map(parse_entity)
        .collect()
}

/// The bot's items with damage, from its own entity data: `id damage` for each stack in its
/// `Inventory` that has a damage component. Stacks are split at `Slot: `, each stack printing
/// its slot before its components and id (best effort: a reader's aid, never the planner's).
pub fn bot_items(snbt: &str) -> Vec<String> {
    let Some(i) = snbt.find("Inventory: [") else {
        return Vec::new();
    };
    snbt[i..]
        .split("Slot: ")
        .skip(1)
        .filter_map(|stack| {
            let id = value_after(stack, "id: ")?.trim_start_matches("minecraft:");
            let d = value_after(stack, "\"minecraft:damage\": ").and_then(number)?;
            Some(format!("{id} damage {d}"))
        })
        .collect()
}

/// What the server held that the bot did not, at the end: each entity within 32 the bot never
/// tracked (no entity of its kind believed within 2 blocks of where the server has it).
pub fn blind(sample: &TruthSample, believed: &[(String, [f64; 3])]) -> Vec<String> {
    sample
        .entities
        .iter()
        .filter_map(|t| {
            let p = t.pos?;
            let tracked = believed.iter().any(|(k, b)| {
                *k == t.kind && (0..3).map(|i| (b[i] - p[i]).powi(2)).sum::<f64>().sqrt() <= 2.0
            });
            (!tracked).then(|| {
                format!(
                    "never tracked: {} at {:.0},{:.0},{:.0}{}",
                    t.kind,
                    p[0],
                    p[1],
                    p[2],
                    t.anger
                        .filter(|a| *a > 0)
                        .map_or_else(String::new, |a| format!(", angry {a}"))
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &str = "Blaze has the following entity data: {Health: 20.0f, id: \"minecraft:blaze\", Pos: [10.5d, 64.0d, -3.25d], Fire: -1s}Enderman has the following entity data: {AngerTime: 300, Health: 40.0f, id: \"minecraft:enderman\", Pos: [1.0d, 70.0d, 2.0d]}";

    #[test]
    fn a_reply_reads_as_entities() {
        let e = parse_reply(REPLY);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].kind, "blaze");
        assert_eq!(e[0].pos, Some([10.5, 64.0, -3.25]));
        assert_eq!(e[0].health, Some(20.0));
        assert_eq!(e[1].kind, "enderman");
        assert_eq!(e[1].anger, Some(300));
    }

    /// A blaze the bot never knew of is a blind spot; one it believed near is not.
    #[test]
    fn an_untracked_entity_is_blind() {
        let s = TruthSample {
            entities: parse_reply(REPLY),
            ..TruthSample::default()
        };
        let believed = vec![("enderman".to_owned(), [1.5, 70.0, 2.0])];
        let b = blind(&s, &believed);
        assert_eq!(b, vec!["never tracked: blaze at 10,64,-3".to_owned()]);
    }

    #[test]
    fn the_bots_worn_items_are_read() {
        let snbt = "{Inventory: [{count: 1, Slot: 0b, components: {\"minecraft:damage\": 120}, id: \"minecraft:stone_pickaxe\"}, {count: 64, Slot: 1b, id: \"minecraft:cobblestone\"}]}";
        assert_eq!(bot_items(snbt), vec!["stone_pickaxe damage 120".to_owned()]);
    }
}
