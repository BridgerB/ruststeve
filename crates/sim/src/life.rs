//! Issue 12a: the server's side of staying alive, each rule read from the 26.1.2 jar with `javap`
//! (brain/ruststeve/design/server-model.md):
//!
//! - air (`LivingEntity.baseTick`): 300; one lost a tick with the eye in water and no water
//!   breathing; at -20 it is set to 0 and the player takes 2 (`drown`); four regained a tick
//!   out of water. Sent as the player's own metadata index 1.
//! - lava and fire (`Entity.baseTick`, `lavaHurt`, `BaseFireBlock.entityInside`): in lava 4
//!   (`lava`) and fire for 15 s; in a fire block 1 (`in_fire`) and fire for 8 s; on fire, 1
//!   (`on_fire`) each 20 ticks; water puts it out. Sent as metadata index 0, bit 0.
//! - hurt (`LivingEntity.hurtServer`): 20 ticks of invulnerability after a hit; in its first
//!   half only the excess over the last hit lands. Each hit costs 0.1 exhaustion.
//! - hunger (`FoodData.tick`): over 4 exhaustion takes a saturation point, else a food point
//!   (not peaceful); full food and saturation heal quickly, 18 food heals 1 each 80 ticks for 6
//!   exhaustion; empty food starves 1 each 80 ticks (on Normal down to 1). Jumps cost 0.05, a
//!   block broken 0.005, a swing landed 0.1. `set_health` is sent when the health or food changes,
//!   or the saturation turns to or from zero (`ServerPlayer.doTick`).
//! - eating: `use_item` with food held (and room for it, or `can_always_eat`) uses it for the
//!   food's ticks, then `FoodData.eat` (food capped at 20, saturation at the food), entity event 9.
//! - melee (`Player.attack`): the held item's damage (base 1 plus its modifier) times
//!   `0.2 + scale² × 0.8`, the scale the ticks since the last swing over `20 / speed` (base 4
//!   plus its modifier); within 3 + 1 of the eye; the mob's own invulnerability; at 0 health
//!   entity event 3 and, 20 ticks on, its removal.
//! - mobs here are placed by a start (a moment's `summon`): no AI, no loot.

use data::{BlockKind, EntityKind, ItemKind};
use protocol::metadata::MetaValue;
use protocol::{EncodeError, S2c, State};

use crate::{PLAYER_ID, Sim, Wire, wire};

/// The air supply's full value and the step it is lost and regained by.
pub const MAX_AIR: i32 = 300;
const AIR_REGAIN: i32 = 4;
/// Fire ticks lava and a fire block set (15 s and 8 s).
const LAVA_FIRE_TICKS: i32 = 300;
const FIRE_BLOCK_TICKS: i32 = 160;
/// Invulnerability after a hit, and the half in which only a larger hit lands.
const INVULNERABLE_TICKS: u32 = 20;
const INVULNERABLE_HALF: u32 = 10;
/// A dead mob is removed this long after its death event (`deathTime`).
const DEATH_TICKS: u64 = 20;
/// Reach for melee: the entity interaction range 3 plus the server's 1 of slack.
const MELEE_REACH: f64 = 3.0 + 1.0;
/// Exhaustion costs (`FoodConstants`).
const EXHAUSTION_JUMP: f32 = 0.05;
const EXHAUSTION_ATTACK: f32 = 0.1;
const EXHAUSTION_HURT: f32 = 0.1;
const EXHAUSTION_MINE: f32 = 0.005;
const EXHAUSTION_HEAL: f32 = 6.0;

/// A mob a start placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mob {
    pub kind: EntityKind,
    pub pos: [f64; 3],
    pub health: f32,
    pub invulnerable: u32,
    pub last_hurt: f32,
    pub dead_at: Option<u64>,
}

/// The player's body as the server keeps it (issue 12a).
#[derive(Clone, Debug, PartialEq)]
pub struct Life {
    pub food: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    food_timer: u32,
    pub air: i32,
    pub fire_ticks: i32,
    pub invulnerable: u32,
    last_hurt: f32,
    /// The food being eaten and the ticks left.
    pub using: Option<(ItemKind, u32)>,
    /// A hunger effect's ticks left and amplifier (a moment's `effects`).
    pub hunger: Option<(u32, u32)>,
    /// Water breathing (the real side of a moment gives it unless the moment says not to).
    pub water_breathing: bool,
    /// Ticks since the last swing (`attackStrengthTicker`).
    pub attack_ticker: u32,
    pub mobs: std::collections::BTreeMap<i32, Mob>,
    /// What the client was last told.
    sent: (f32, i32, bool),
    sent_air: i32,
    sent_fire: bool,
    pub kills: u64,
}

impl Default for Life {
    fn default() -> Self {
        Self {
            food: 20,
            saturation: 5.0,
            exhaustion: 0.0,
            food_timer: 0,
            air: MAX_AIR,
            fire_ticks: 0,
            invulnerable: 0,
            last_hurt: 0.0,
            using: None,
            hunger: None,
            water_breathing: false,
            attack_ticker: 0,
            mobs: std::collections::BTreeMap::new(),
            sent: (20.0, 20, false),
            sent_air: MAX_AIR,
            sent_fire: false,
            kills: 0,
        }
    }
}

impl Life {
    /// `hurt` sent a `set_health` itself.
    pub(crate) fn note_health_sent(&mut self, health: f32) {
        self.sent = (health, self.food, self.saturation == 0.0);
    }
}

fn kind_at(sim: &Sim, (x, y, z): (i32, i32, i32)) -> Option<BlockKind> {
    sim.state_at(x, y, z).and_then(data::block_of_state)
}

impl Sim {
    /// Place a mob (a start's `summon`), with the game's max health.
    pub fn summon(&mut self, kind: EntityKind, pos: [f64; 3]) -> i32 {
        let id = self.survival.next_id();
        let health = data::generated::mobs::MOBS
            .iter()
            .find(|m| m.entity == kind)
            .map_or(20.0, |m| m.max_health as f32);
        self.survival.life.mobs.insert(
            id,
            Mob {
                kind,
                pos,
                health,
                invulnerable: 0,
                last_hurt: 0.0,
                dead_at: None,
            },
        );
        id
    }

    /// The mobs' `add_entity`, for the join.
    pub(crate) fn mobs_added(&self) -> Result<Vec<Wire>, EncodeError> {
        self.survival
            .life
            .mobs
            .iter()
            .filter(|(_, m)| m.dead_at.is_none())
            .map(|(id, m)| {
                let mut uuid = [0u8; 16];
                uuid[12..].copy_from_slice(&id.to_be_bytes());
                wire(
                    State::Play,
                    &S2c::AddEntity {
                        id: *id,
                        uuid,
                        kind: m.kind.id(),
                        x: m.pos[0],
                        y: m.pos[1],
                        z: m.pos[2],
                    },
                )
            })
            .collect()
    }

    fn box_touches(&self, kinds: &[BlockKind]) -> bool {
        let b = physics::aabb(self.pos);
        let cells = |lo: f64, hi: f64| (lo.floor() as i32)..=((hi - 1e-7).floor() as i32);
        cells(b[0], b[3]).any(|x| {
            cells(b[1], b[4]).any(|y| {
                cells(b[2], b[5])
                    .any(|z| kind_at(self, (x, y, z)).is_some_and(|k| kinds.contains(&k)))
            })
        })
    }

    /// Damage the player as `hurtServer` does: invulnerability first, then the hit, its
    /// exhaustion, and the packets.
    pub(crate) fn hurt_player(
        &mut self,
        damage: f32,
        damage_type: &str,
        death_key: &str,
    ) -> Result<Vec<Wire>, EncodeError> {
        if self.health <= 0.0 {
            return Ok(Vec::new());
        }
        let life = &mut self.survival.life;
        let dealt = if life.invulnerable > INVULNERABLE_HALF {
            if damage <= life.last_hurt {
                return Ok(Vec::new());
            }
            damage - life.last_hurt
        } else {
            life.invulnerable = INVULNERABLE_TICKS;
            damage
        };
        life.last_hurt = damage;
        life.exhaustion += EXHAUSTION_HURT;
        self.hurt(dealt, damage_type, death_key)
    }

    /// The player's tick of air, fire, hunger, eating and the mobs.
    pub(crate) fn tick_life(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        if self.health <= 0.0 {
            return Ok(out);
        }
        let peaceful = self.config.peaceful;
        {
            let life = &mut self.survival.life;
            life.attack_ticker += 1;
            life.invulnerable = life.invulnerable.saturating_sub(1);
            if let Some((t, amp)) = life.hunger {
                life.exhaustion += 0.005 * (amp as f32 + 1.0);
                life.hunger = (t > 1).then_some((t - 1, amp));
            }
        }
        // Air.
        let eye = [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]];
        let eye_cell = (
            eye[0].floor() as i32,
            eye[1].floor() as i32,
            eye[2].floor() as i32,
        );
        let under = self.eye_in_water(eye[1], eye_cell);
        if under && !self.survival.life.water_breathing {
            self.survival.life.air -= 1;
            if self.survival.life.air <= -20 {
                self.survival.life.air = 0;
                out.extend(self.hurt_player(2.0, "drown", "death.attack.drown")?);
            }
        } else if self.survival.life.air < MAX_AIR {
            self.survival.life.air = (self.survival.life.air + AIR_REGAIN).min(MAX_AIR);
        }
        // Lava, fire blocks, burning.
        let in_water = self.box_touches(&[BlockKind::Water, BlockKind::BubbleColumn]);
        if in_water {
            self.survival.life.fire_ticks = 0;
        }
        if self.box_touches(&[BlockKind::Lava]) {
            self.survival.life.fire_ticks = self.survival.life.fire_ticks.max(LAVA_FIRE_TICKS);
            out.extend(self.hurt_player(4.0, "lava", "death.attack.lava")?);
        } else if self.box_touches(&[BlockKind::Fire, BlockKind::SoulFire]) {
            self.survival.life.fire_ticks = self.survival.life.fire_ticks.max(FIRE_BLOCK_TICKS);
            out.extend(self.hurt_player(1.0, "in_fire", "death.attack.inFire")?);
        } else if self.survival.life.fire_ticks > 0 {
            if self.survival.life.fire_ticks % 20 == 0 {
                out.extend(self.hurt_player(1.0, "on_fire", "death.attack.onFire")?);
            }
            self.survival.life.fire_ticks -= 1;
        }
        // Eating.
        if let Some((item, left)) = self.survival.life.using {
            if left > 1 {
                self.survival.life.using = Some((item, left - 1));
            } else {
                self.survival.life.using = None;
                out.extend(self.finish_eating(item)?);
            }
        }
        // Hunger (`FoodData.tick`).
        out.extend(self.tick_food(peaceful)?);
        // Mobs: invulnerability, and the dead removed.
        let tick = self.ticks;
        let mut gone = Vec::new();
        for (id, m) in self.survival.life.mobs.iter_mut() {
            m.invulnerable = m.invulnerable.saturating_sub(1);
            if m.dead_at.is_some_and(|d| tick >= d + DEATH_TICKS) {
                gone.push(*id);
            }
        }
        for id in &gone {
            self.survival.life.mobs.remove(id);
        }
        if !gone.is_empty() {
            out.push(wire(State::Play, &S2c::RemoveEntities { ids: gone })?);
        }
        out.extend(self.sync_life()?);
        Ok(out)
    }

    fn tick_food(&mut self, peaceful: bool) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let hurt = self.health < 20.0;
        let life = &mut self.survival.life;
        if life.exhaustion > 4.0 {
            life.exhaustion -= 4.0;
            if life.saturation > 0.0 {
                life.saturation = (life.saturation - 1.0).max(0.0);
            } else if !peaceful {
                life.food = (life.food - 1).max(0);
            }
        }
        if life.saturation > 0.0 && hurt && life.food >= 20 {
            life.food_timer += 1;
            if life.food_timer >= 10 {
                let amount = life.saturation.min(6.0);
                life.exhaustion += amount;
                life.food_timer = 0;
                self.health = (self.health + amount / 6.0).min(20.0);
            }
        } else if life.food >= 18 && hurt {
            life.food_timer += 1;
            if life.food_timer >= 80 {
                life.exhaustion += EXHAUSTION_HEAL;
                life.food_timer = 0;
                self.health = (self.health + 1.0).min(20.0);
            }
        } else if life.food <= 0 {
            life.food_timer += 1;
            if life.food_timer >= 80 {
                life.food_timer = 0;
                // Normal: down to 1 health (`Difficulty.NORMAL`).
                if self.health > 1.0 {
                    out.extend(self.hurt_player(1.0, "starve", "death.attack.starve")?);
                }
            }
        } else {
            life.food_timer = 0;
        }
        Ok(out)
    }

    fn finish_eating(&mut self, item: ItemKind) -> Result<Vec<Wire>, EncodeError> {
        let Some(food) = data::generated::item_props::food_of(item) else {
            return Ok(Vec::new());
        };
        let held = self.survival.held_slot_index();
        match self.survival.slots.get(held).copied().flatten() {
            // `Consumable.finishConsume` then `ItemUtils.createFilledResult`: the remainder
            // (a glass bottle) takes the emptied hand, else goes into the inventory.
            Some((k, n)) if k == item => {
                if let Some(s) = self.survival.slots.get_mut(held) {
                    *s = match (n > 1, food.remainder) {
                        (true, _) => Some((k, n - 1)),
                        (false, Some(r)) => Some((r, 1)),
                        (false, None) => None,
                    };
                }
                if n > 1
                    && let Some(r) = food.remainder
                {
                    self.survival.give((r, 1));
                }
            }
            _ => return Ok(Vec::new()),
        }
        let life = &mut self.survival.life;
        life.food = (life.food + i32::from(food.nutrition)).min(20);
        life.saturation = (life.saturation + food.saturation).min(life.food as f32);
        Ok(vec![wire(
            State::Play,
            &S2c::EntityEvent {
                id: PLAYER_ID,
                status: 9,
            },
        )?])
    }

    /// `use_item`: start eating the held food when there is room for it.
    pub(crate) fn on_use_item(&mut self, sequence: i32) -> Result<Vec<Wire>, EncodeError> {
        if let Some((item, _)) = self.survival.held_stack()
            && let Some(food) = data::generated::item_props::food_of(item)
            && (self.survival.life.food < 20 || food.always)
        {
            self.survival.life.using = Some((item, u32::from(food.ticks)));
        }
        Ok(vec![wire(State::Play, &S2c::BlockChangedAck { sequence })?])
    }

    /// `player_action` status 5: the use key let go.
    pub(crate) fn on_release_use(&mut self) {
        self.survival.life.using = None;
    }

    /// A jump (`jumpFromGround`, from a move that leaves the ground going up).
    pub(crate) fn jumped(&mut self) {
        self.survival.life.exhaustion += EXHAUSTION_JUMP;
    }

    /// A block broken costs exhaustion (`causeFoodExhaustion` in `playerDestroy`).
    pub(crate) fn mined(&mut self) {
        self.survival.life.exhaustion += EXHAUSTION_MINE;
    }

    /// `attack`: a placed mob in reach takes the held item's damage scaled by the swing's
    /// charge.
    pub(crate) fn on_attack(&mut self, entity: i32) -> Result<Vec<Wire>, EncodeError> {
        let eye = [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]];
        let held = self.survival.held_stack().map(|(k, _)| k);
        let weapon = held.and_then(data::generated::item_props::weapon_of);
        let base = 1.0 + weapon.map_or(0.0, |w| w.damage);
        let speed = 4.0 + weapon.map_or(0.0, |w| w.speed);
        let delay = 20.0 / speed.max(0.01);
        let life = &mut self.survival.life;
        let scale = ((f64::from(life.attack_ticker) + 0.5) / delay).clamp(0.0, 1.0);
        let damage = (base * (0.2 + scale * scale * 0.8)) as f32;
        let Some(m) = life.mobs.get_mut(&entity).filter(|m| m.dead_at.is_none()) else {
            return Ok(Vec::new());
        };
        // The distance from the eye to the mob's box (0.6 wide, 1.95 tall for a zombie-sized one).
        let (hw, h) = (0.3, 1.95);
        let axis = |e: f64, lo: f64, hi: f64| (lo - e).max(0.0).max(e - hi);
        let d = [
            axis(eye[0], m.pos[0] - hw, m.pos[0] + hw),
            axis(eye[1], m.pos[1], m.pos[1] + h),
            axis(eye[2], m.pos[2] - hw, m.pos[2] + hw),
        ];
        if d.iter().map(|v| v * v).sum::<f64>() > MELEE_REACH * MELEE_REACH {
            self.violations
                .push(format!("attack {entity} out of reach"));
            return Ok(Vec::new());
        }
        life.attack_ticker = 0;
        life.exhaustion += EXHAUSTION_ATTACK;
        let dealt = if m.invulnerable > INVULNERABLE_HALF {
            if damage <= m.last_hurt {
                return Ok(Vec::new());
            }
            damage - m.last_hurt
        } else {
            m.invulnerable = INVULNERABLE_TICKS;
            damage
        };
        m.last_hurt = damage;
        m.health = (m.health - dealt).max(0.0);
        let health = m.health;
        let mut out = vec![wire(
            State::Play,
            &S2c::SetEntityData {
                id: entity,
                entries: vec![(9, MetaValue::Float(health))],
            },
        )?];
        if health <= 0.0 {
            m.dead_at = Some(self.ticks);
            life.kills += 1;
            out.push(wire(
                State::Play,
                &S2c::EntityEvent {
                    id: entity,
                    status: 3,
                },
            )?);
        }
        Ok(out)
    }

    /// What changed for the client: `set_health` when the health or food changed or the
    /// saturation turned to or from zero; the air and on-fire metadata when they changed.
    fn sync_life(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let life = &mut self.survival.life;
        let now = (self.health, life.food, life.saturation == 0.0);
        if now != life.sent {
            life.sent = now;
            out.push(wire(
                State::Play,
                &S2c::SetHealth {
                    health: self.health,
                    food: life.food,
                    saturation: life.saturation,
                },
            )?);
        }
        let mut entries = Vec::new();
        if life.air != life.sent_air {
            life.sent_air = life.air;
            entries.push((1, MetaValue::Int(life.air)));
        }
        let fire = life.fire_ticks > 0;
        if fire != life.sent_fire {
            life.sent_fire = fire;
            entries.push((0, MetaValue::Byte(u8::from(fire))));
        }
        if !entries.is_empty() {
            out.push(wire(
                State::Play,
                &S2c::SetEntityData {
                    id: PLAYER_ID,
                    entries,
                },
            )?);
        }
        Ok(out)
    }
}
