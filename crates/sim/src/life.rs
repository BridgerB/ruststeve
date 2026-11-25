//! Issue 12a: the server's side of staying alive, each rule read from the 26.1.2 jar with `javap`
//! (brain/ruststeve/design/server-model.md):
//!
//! - air (`LivingEntity.baseTick`): 300; one lost a tick with the eye in water and no water
//!   breathing; at -20 it is set to 0 and the player takes 2 (`drown`); four regained a tick
//!   out of water. Sent as the player's own metadata index 1.
//! - lava and fire (`Entity.baseTick`, `lavaHurt`, `BaseFireBlock.entityInside`): in lava 4
//!   (`lava`) and fire for 15 s; in a fire block 1 (`in_fire`) and fire for 8 s; on fire, 1
//!   (`on_fire`) each 20 ticks; water puts it out. Sent as metadata index 0, bit 0.
//! - magma (`Entity.applyEffectsFromBlocks`, `MagmaBlock.stepOn`): on the ground, the block 0.2
//!   under the feet (`getOnPosLegacy`) a magma block and the player not stepping carefully
//!   (`isShiftKeyDown`): 1 (`hot_floor`) a tick, which the invulnerability below makes 1 each
//!   10 ticks. `hot_floor` is in `is_fire`: fire resistance refuses it.
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
//! - mobs here are placed by a start (a moment's `summon`): no AI, no loot, unless given a
//!   `Brain` (issue 16's fight arenas): `Melee` walks straight at the player over ground that
//!   holds it and hits in reach every 20 ticks (`MeleeAttackGoal`), a wither skeleton's hit
//!   leaving the wither effect. No navigation.
//! - a blaze with `Brain::Blaze` (`Blaze$BlazeAttackGoal.tick`, javap): with the player in
//!   sight and within its follow range 48, it charges 60 ticks, looses three small fireballs 6
//!   ticks apart, and rests 100; within 2 blocks it hits for 6 every 20.
//!   It flies only up and down (`Blaze.customServerAiStep`, `aiStep`): while the player's eye is
//!   over its own plus an offset (`triangle(0.5, 6.891)`, drawn again every
//!   100 ticks) it rises, `vy += (0.3 - vy) × 0.3`; else it sinks, its fall cut to 0.6 a tick.
//!   Sideways it moves as its goals say (`Blaze.registerGoals`): at a seen target within 2
//!   blocks, and at one lost from sight for under 5 ticks (`BlazeAttackGoal`); with no target
//!   (none seen for 60 ticks, `TargetGoal`'s memory) it strolls now and then to a place
//!   within 10 blocks (`WaterAvoidingRandomStrollGoal` at probability 0: a land position
//!   10 out and 7 up or down, one start in 120 ticks). While it has a target in sight
//!   farther than 2 blocks it does not move: the attack goal holds the move flag. It goes in
//!   a straight line at 0.1165 blocks a tick with ground under it and 0.051 in the air (its
//!   0.23 speed through `LivingEntity.travel`: squared over 0.454 on the ground, 0.02 of
//!   flying speed over 0.09 in the air); the game finds a path.
//! - knockback (`LivingEntity.knockback`, strength 0.4): a mob's hit or a fireball's sends the
//!   player's motion as 0.4 away from it on the plane and 0.4 up when on the ground. The
//!   player's fresh hit (not one inside the invulnerable half) knocks a mob the same way:
//!   half its motion kept plus 0.4 away on the plane, then moved a tick at a time and cut by
//!   0.91 in the air or 0.546 on a 0.6 floor (`LivingEntity.travel`), zeroed under 0.003. A
//!   knock from the ground lifts the mob 0.4 (`min(0.4, vy / 2 + 0.4)`), which under gravity
//!   0.08 and drag 0.98 keeps it aloft 11 ticks: the air's drag holds that long, so a walker
//!   goes about 3 blocks in 11 ticks (clojurecraft measured the same, brain mc-mob-facts) and
//!   a blaze in the air about 4.4. A blaze's lift is not drawn (its height is its flight's). A
//!   walker's is: lifted `min(0.4, vy / 2 + 0.4)` (0.3608 standing), it falls under gravity
//!   0.08 and drag 0.98 where nothing holds it, so a knock carries it off a bridge's edge, and
//!   it lands with `ceil(fallen - 3)` of fall damage (`tick_falls`). The player's lift is
//!   the same rule on the server's own vy. A `Still` mob is not moved. A sprint's extra knock
//!   is not modelled.
//! - `Life::hurt_by` sums the damage each type dealt, for a test to name what a death was.
//! - a fire set with updates (a fireball's, flint and steel's) keeps its own clock
//!   (`FireBlock.tick`, every 30 to 39 ticks): on netherrack or magma it burns on; elsewhere
//!   it ages by `nextInt(3) / 2` and goes out once older than 3 (about 25 s), nothing
//!   flammable being taken to stand by it. The infiniburn floors are the jar's
//!   `#infiniburn_nether` (= `#infiniburn_overworld`: netherrack and magma_block; probe
//!   45359635431). Fires the world began with (a stage's, a capture's) get their clocks at the
//!   first tick (`wake_fires`) and age the same way.
//! - a spawner (`BaseSpawner`, javap): with the player within 16 it counts down (20 at first,
//!   then 200 plus up to 599); at 0, unless 6 of its mob are alive within 4 of it, it tries 4
//!   places (`(rand - rand) × 4` on x and z, -1 to 1 on y) and spawns where the body is free.
//!   The model holds no light, so no light rule is applied (a blaze's spawn placement asks
//!   for any light; not read from the jar here).
//! - a small fireball (`AbstractHurtingProjectile`, `SmallFireball`): from rest it gains 0.1 a
//!   tick along its aim and keeps 0.95 of its speed; the aim is at the player's middle with
//!   a triangular spread of `2.297 × sqrt(sqrt(distance²)) × 0.5` on x and z; a hit takes 5
//!   (`fireball`, cut by armour) and sets the player alight for 5 s; a block hit leaves fire in
//!   the cell before it when that has a floor.
//! - a shield (`Items.SHIELD`'s `BlocksAttacks`, probe 45359635431: delay 0.25 s, one
//!   reduction of base 0 factor 1 within 90 degrees, item damage threshold 3 base 1 factor 1,
//!   bypassed by `#bypasses_shield`, durability 336): held in either hand and used for 5 ticks
//!   or more, it stops whole a fireball or a mob's blow (`mob_attack`) whose source position
//!   (the fireball's own, the mob's) is in the half of the plane the player faces: no damage,
//!   no fire, no wither, the block sound. `in_fire`, `lava`, `hot_floor`, `on_fire`, `wither`
//!   are in `#bypasses_shield`. Each blocked hit of 3 or more wears it `floor(1 + damage)` (6 a
//!   fireball), sent as its `damage` component; at 336 it breaks. A blocked blow knocks the
//!   mob 0.5 back (`blockedByItem`); a blow from a weapon with `disable_blocking_for_seconds`
//!   (`Melee::disables_shield`) stops the use and cools the shield down (`BlocksAttacks.disable`:
//!   seconds × 20). A blocked hit is still a hit of 0 to `hurtServer`: outside the invulnerable
//!   half it starts 20 fresh ticks with `lastHurt` 0, and the player's client is not knocked.
//! - a fireball or a blow that lands nothing (inside the invulnerable half, no excess) leaves no
//!   fire (`SmallFireball.onHitEntity` puts the fire ticks back) and no wither.
//! - a blaze's touch (`Mob.doHurtTarget`; `Blaze` does not override it) is its `attack_damage`
//!   6 as `mob_attack`, every 20 ticks, and sets nothing alight: fire from a blow comes only from
//!   a weapon's enchantment (`doPostAttackEffects`), and a blaze holds none.
//! - armour also cuts `lava`, `in_fire` and `hot_floor` (not in `#bypasses_armor`).
//! - armour (`CombatRules.getDamageAfterArmorAbsorb`): a mob's hit is cut by
//!   `min(20, max(armor / 5, armor - 4 × damage / 8)) / 25`; `Life::armor` is set by a test (the
//!   model wears nothing).
//! - wither (`MobEffects.WITHER`, amplifier 0): 1 (`wither`) each 40 ticks while it lasts.

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
/// How far under the feet the block stood on is read (`Entity.getOnPosLegacy`'s 0.2).
const ON_POS_BELOW: f64 = 0.2;
/// What a magma block does to feet on it (`MagmaBlock.stepOn`'s 1.0).
const MAGMA_DAMAGE: f32 = 1.0;
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

/// Ticks between a melee mob's hits (`MeleeAttackGoal.resetAttackCooldown`: 20).
const MELEE_EVERY: u64 = 20;
/// Ticks between the wither effect's hits at amplifier 0 (`WitherMobEffect`: 40 >> amplifier).
const WITHER_EVERY: u32 = 40;
/// Armour's ceiling and the toughness-free divisor of `CombatRules.getDamageAfterArmorAbsorb`.
const ARMOR_CAP: f32 = 20.0;

/// How a melee mob comes and hits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Melee {
    /// Blocks a tick over the ground.
    pub speed: f64,
    pub damage: f32,
    /// Centre to centre on the plane, at which it hits and stops coming.
    pub reach: f64,
    /// Ticks of the wither effect a hit leaves.
    pub wither_ticks: u32,
    /// Ticks a raised shield it strikes is disabled for (an axe's blow; 0 for none).
    pub disables_shield: u32,
}

impl Melee {
    /// A wither skeleton on Normal (probe 45359635431). Speed: its `movement_speed` 0.25
    /// (`AbstractSkeleton.createAttributes`) times the melee goal's 1.2 is both the mob's speed
    /// and its forward input, so the ground speed settles at 0.3 x 0.3 / (1 - 0.546), 0.198
    /// blocks a tick (a zombie's 0.23 gives the 2.3 blocks a second it is known for). Damage
    /// 8: `attack_damage` set to 4 in `WitherSkeleton.finalizeSpawn`, plus its stone sword's 4
    /// (`Items.STONE_SWORD`: `sword(STONE, 3, -2.4)`, stone's bonus 1). The wither effect for
    /// 200 ticks when the blow lands (`WitherSkeleton.doHurtTarget`), 1 each 40
    /// (`WitherMobEffect`). Reach about 1.5 (its box 0.7 wide, the attack box's 0.83 beyond it,
    /// to the player's 0.3).
    pub fn wither_skeleton() -> Self {
        Self {
            speed: 0.198,
            damage: 8.0,
            reach: 1.5,
            wither_ticks: 200,
            disables_shield: 0,
        }
    }

    /// A piglin brute (bastions, not fortresses: here for a shield's test). `attack_damage` 7,
    /// `movement_speed` 0.35 (`PiglinBrute.createAttributes`, probe 45359635431) and its
    /// golden axe (`populateDefaultEquipmentSlots`), whose damage is the generated item
    /// table's. An axe disables a shield it strikes for 5 s (an axe's `weapon` component,
    /// `disable_blocking_for_seconds`; not read from the jar here: `Item$Properties.axe`) times
    /// the shield's `disable_cooldown_scale` 1: 100 ticks. Speed 0.35 × 1.0 (its melee goal's,
    /// assumed) settles at 0.35 × 0.35 / 0.454.
    pub fn piglin_brute() -> Self {
        let axe = data::generated::item_props::weapon_of(ItemKind::GoldenAxe)
            .map_or(6.0, |w| w.damage as f32);
        Self {
            speed: 0.27,
            damage: 7.0 + axe,
            reach: 1.5,
            wither_ticks: 0,
            disables_shield: 100,
        }
    }
}

/// What a placed mob does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brain {
    /// Stands (or hovers) where it was put.
    Still,
    Melee(Melee),
    /// A blaze's ranged attack: ticks until its next step, and the step of its volley (0
    /// resting, 1 charged, 2 to 4 a fireball each).
    Blaze {
        attack_time: i32,
        step: u8,
        /// Its upward speed, and the height over its own eye it lets the player's eye be.
        vy: f64,
        offset: f64,
        /// Ticks since it last saw the player, and where a stroll is taking it (x, z).
        unseen: u32,
        stroll: Option<[f64; 2]>,
    },
    /// An enderman (issue 17, `crate::enderman`).
    Enderman(crate::enderman::Ender),
}

impl Brain {
    /// A blaze about to begin its first charge.
    pub fn blaze() -> Self {
        Brain::Blaze {
            attack_time: 0,
            step: 0,
            vy: 0.0,
            offset: 0.5,
            unseen: 0,
            stroll: None,
        }
    }
}

/// A small fireball in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fireball {
    pub id: i32,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    /// Its aim, a unit vector: what it accelerates along.
    pub dir: [f64; 3],
    /// Where it was loosed from (a shield stops what comes from the side it faces).
    pub from: [f64; 3],
    pub age: u32,
}

/// `Blaze$BlazeAttackGoal`: the charge, the gap between fireballs, the rest, the melee gap.
const BLAZE_CHARGE: i32 = 60;
const BLAZE_GAP: i32 = 6;
const BLAZE_REST: i32 = 100;
const BLAZE_MELEE_EVERY: i32 = 20;
/// A blaze's `follow_range` and `attack_damage` (`Blaze.createAttributes`).
const BLAZE_RANGE: f64 = 48.0;
const BLAZE_MELEE: f32 = 6.0;
/// `Blaze.customServerAiStep`: the rise's target speed and gain (both 0.3), the cut to a fall
/// (`aiStep`: 0.6), how often the height offset is drawn again, and its eye over its feet
/// (0.85 of 1.8).
const BLAZE_RISE: f64 = 0.3;
const BLAZE_FALL: f64 = 0.6;
const BLAZE_OFFSET_EVERY: u64 = 100;
/// The spread of its height offset (`triangle(0.5, 6.891)`; read from the jar in probe
/// 45344729907).
const BLAZE_OFFSET_SPREAD: f64 = 6.891;
const BLAZE_EYE: f64 = 1.53;
/// A blaze's sideways movement: how long a target unseen is remembered (`TargetGoal`: 60),
/// how long it is still come after (`BlazeAttackGoal`: `lastSeen < 5`), a stroll's chance a
/// tick (`RandomStrollGoal`: 1 in 120) and reach (`LandRandomPos.getPos(mob, 10, 7)`), and its
/// pace on the ground and in the air (derived from its 0.23 speed; the module's note).
const BLAZE_MEMORY: u32 = 60;
const BLAZE_CHASE: u32 = 5;
const BLAZE_STROLL_EVERY: f64 = 120.0;
const BLAZE_STROLL: f64 = 10.0;
const BLAZE_WALK: f64 = 0.1165;
const BLAZE_FLY: f64 = 0.051;
/// `LivingEntity.knockback`'s strength from a mob's hit.
const KNOCKBACK: f64 = 0.4;
/// `LivingEntity.travel`: horizontal motion kept a tick in the air (0.91), and on a floor of
/// friction 0.6 (netherrack, nether bricks: 0.6 × 0.91); motion under 0.003 is zeroed
/// (`LivingEntity.aiStep`).
const DRAG_AIR: f64 = 0.91;
const DRAG_GROUND: f64 = 0.546;
const MOTION_FLOOR: f64 = 0.003;
/// Ticks a knock's 0.4 lift keeps a mob off the floor: y 0.4, 0.71, 0.94, 1.09, 1.15, 1.14,
/// 1.05, 0.88, 0.63, 0.31, then below 0 on the 11th (vy -= 0.08, × 0.98 a tick).
const KNOCK_ALOFT: u32 = 11;
/// `BaseSpawner`: the first delay, the least and the span of the later ones, tries a cycle,
/// the most of its mob near, how near a player must be, and the spawn range.
const SPAWNER_FIRST: i32 = 20;
const SPAWNER_MIN: i32 = 200;
const SPAWNER_SPAN: f64 = 600.0;
const SPAWNER_TRIES: u32 = 4;
const SPAWNER_NEARBY: usize = 6;
const SPAWNER_PLAYER: f64 = 16.0;
const SPAWNER_RANGE: f64 = 4.0;

/// A spawner block's own state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spawner {
    pub pos: (i32, i32, i32),
    pub kind: EntityKind,
    pub brain: Brain,
    pub delay: i32,
    /// Mobs it has spawned.
    pub spawned: u64,
}

/// The fireball's spread factor (`2.297` in the goal).
const BLAZE_SPREAD: f64 = 2.297;
/// `AbstractHurtingProjectile`: `accelerationPower` and `getInertia`.
const FIREBALL_POWER: f64 = 0.1;
const FIREBALL_INERTIA: f64 = 0.95;
/// `SmallFireball.onHitEntity`: 5 damage and 5 s alight.
const FIREBALL_DAMAGE: f32 = 5.0;
const FIREBALL_FIRE_TICKS: i32 = 100;
/// A fireball that has hit nothing is dropped after this long (the model's own bound).
const FIREBALL_LIFE: u32 = 200;
/// How far a projectile's hit test reaches past the target's box (`ProjectileUtil`: 0.3).
const PROJECTILE_MARGIN: f64 = 0.3;
/// Ticks a shield must be up before it blocks (`block_delay_seconds` 0.25).
const SHIELD_DELAY: u64 = 5;
/// The shield's wear for a blocked hit (`BlocksAttacks$ItemDamageFunction`, the shield's
/// `item_damage`): nothing under the threshold 3, else `floor(base 1 + factor 1 × damage)`:
/// 6 a fireball, 7 a blaze's touch, of a new shield's 336.
const SHIELD_WEAR_THRESHOLD: f32 = 3.0;
const SHIELD_WEAR_BASE: f32 = 1.0;
const SHIELD_WEAR_FACTOR: f32 = 1.0;
/// `LivingEntity.blockedByItem`: a mob whose blow a shield stopped is knocked 0.5 away.
const BLOCKED_KNOCK: f64 = 0.5;
/// The sound source players' sounds go out on (`SoundSource.PLAYERS`).
const SOURCE_PLAYERS: i32 = 7;
/// The steps a flight or a line of sight is sampled at, in blocks.
const SAMPLE: f64 = 0.2;

/// A fire block a fireball left, and its own clock (`FireBlock.tick`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fire {
    pub pos: (i32, i32, i32),
    pub age: u8,
    /// The tick of its next scheduled tick.
    pub next: u64,
}

/// `FireBlock.getFireTickDelay`: 30 plus up to 9.
const FIRE_DELAY: u64 = 30;
const FIRE_DELAY_SPAN: f64 = 10.0;
/// `FireBlock.tick`: off an infiniburn floor and with nothing flammable beside it, a fire
/// older than 3 goes out.
const FIRE_OLD: u8 = 3;

/// A mob a start placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mob {
    pub brain: Brain,
    /// The tick its next hit may land.
    pub next_hit: u64,
    pub kind: EntityKind,
    pub pos: [f64; 3],
    pub health: f32,
    pub invulnerable: u32,
    pub last_hurt: f32,
    pub dead_at: Option<u64>,
    /// Whether its death drops its loot table (a parity row's mob is summoned without).
    pub loot: bool,
    /// The sideways motion a player's hit left it with (`LivingEntity.knockback`), x and z,
    /// spent a tick at a time and cut by friction, and the ticks the knock keeps it aloft.
    pub push: [f64; 2],
    pub aloft: u32,
    /// A walker's upward speed (a knock's lift, then gravity) and how far it has fallen since
    /// it last stood: a walker knocked off an edge falls and takes the fall's damage.
    pub vy: f64,
    pub fallen: f64,
}

impl Mob {
    /// `LivingEntity.knockback(strength, …)` away along `away` (x, z): half the sideways motion
    /// kept plus `strength`; a walker on its feet lifted `min(0.4, vy / 2 + 0.4)` (its standing
    /// vy is a tick of gravity, -0.0784: 0.3608), one in the air keeps its vy.
    fn knocked(&mut self, away: [f64; 2], strength: f64) {
        let len = away[0].hypot(away[1]).max(1e-9);
        self.push = [
            self.push[0] / 2.0 + away[0] / len * strength,
            self.push[1] / 2.0 + away[1] / len * strength,
        ];
        self.aloft = KNOCK_ALOFT;
        if matches!(self.brain, Brain::Melee(_)) && self.vy == 0.0 {
            self.vy = (STANDING_VY / 2.0 + KNOCKBACK).min(KNOCKBACK);
        }
    }
}

/// A body on the ground carries a tick of gravity as its vy (`-0.08 × 0.98`).
const STANDING_VY: f64 = -physics::GRAVITY * physics::VERTICAL_DRAG;
/// `safe_fall_distance` (the attribute's default, 3).
const SAFE_FALL: f64 = 3.0;

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
    /// The fire resistance effect: fire, lava and burning do no damage (`LivingEntity.hurt`
    /// refuses an `is_fire` source), though lava still slows and sets the body alight.
    pub fire_resistance: bool,
    /// The shift key as the server holds it (`isShiftKeyDown`), set by the `player_input`
    /// packet the bot's world sends when its keys change: sneaking over magma spares the bot.
    pub sneaking: bool,
    /// Armour points worn (iron helmet, chestplate and leggings: 2 + 6 + 5). The model wears
    /// nothing: a test sets it.
    pub armor: f32,
    /// Ticks of the wither effect left.
    pub wither_ticks: u32,
    /// Small fireballs in flight.
    pub fireballs: Vec<Fireball>,
    /// Fires the fireballs left, ageing until they go out.
    pub fires: Vec<Fire>,
    /// Of those, the ones gone out.
    pub fires_out: u64,
    /// Whether the world's own fires were given their clocks (`Sim::wake_fires`).
    pub fires_woken: bool,
    /// The spawners that run (`Sim::add_spawner`).
    pub spawners: Vec<Spawner>,
    /// The tick a shield went up (`use_item` with a shield in either hand), until the use key
    /// is let go.
    pub blocking_since: Option<u64>,
    /// The menu slot of the shield raised (the held one, else the off hand, 45).
    pub blocking_slot: usize,
    /// The wear each shield holds, by menu slot (`damage` of a new shield's 336). Kept by slot:
    /// a shield moved to another slot loses it (the model's limit; a raised shield stays put).
    pub shield_wear: std::collections::BTreeMap<usize, i32>,
    /// The tick before which a shield cannot be raised (an axe's blow disabled it).
    pub shield_cooldown_until: u64,
    /// Shields worn out, and mobs' blows a shield stopped.
    pub shields_broken: u64,
    pub blows_blocked: u64,
    /// Fireballs a shield stopped, and fireballs that hit.
    pub fireballs_blocked: u64,
    pub fireballs_hit: u64,
    /// Of those hits, the ones that dealt damage (what a race's damage events count).
    pub fireballs_landed: u64,
    /// Fireballs loosed.
    pub fireballs_fired: u64,
    /// The motion a hit this tick knocks the player with, sent in place of the server's own.
    pub knock: Option<[f64; 3]>,
    /// Ticks since the last swing (`attackStrengthTicker`).
    pub attack_ticker: u32,
    pub mobs: std::collections::BTreeMap<i32, Mob>,
    /// What the client was last told.
    sent: (f32, i32, bool),
    sent_air: i32,
    sent_fire: bool,
    pub kills: u64,
    /// Fresh hits on mobs that knocked them back.
    pub mobs_knocked: u64,
    /// Walkers hurt by a fall (knocked off an edge).
    pub mobs_fell: u64,
    /// Damage dealt to the player by damage type (after armour and invulnerability), so a
    /// test can say what a death was made of.
    pub hurt_by: std::collections::BTreeMap<String, f32>,
    /// Endermen the world spawned (issue 17), and whether it spawns them (`Sim::with_natural_spawns`).
    pub ender_spawns: u64,
    pub natural_spawns: bool,
    /// The mob whose blow is being dealt, for the damage event's cause and direct ids (issue 18:
    /// an enderman's; other mobs' blows still send none, so issue 16's fights are unchanged).
    pub blow_cause: Option<i32>,
    /// Blows that landed on the player, by the kind of mob that dealt them (issue 18).
    pub hurt_by_mob: std::collections::BTreeMap<String, u32>,
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
            fire_resistance: false,
            sneaking: false,
            armor: 0.0,
            wither_ticks: 0,
            fireballs: Vec::new(),
            fires: Vec::new(),
            fires_out: 0,
            fires_woken: false,
            spawners: Vec::new(),
            blocking_since: None,
            blocking_slot: 45,
            shield_wear: std::collections::BTreeMap::new(),
            shield_cooldown_until: 0,
            shields_broken: 0,
            blows_blocked: 0,
            fireballs_blocked: 0,
            fireballs_hit: 0,
            fireballs_landed: 0,
            fireballs_fired: 0,
            knock: None,
            attack_ticker: 0,
            mobs: std::collections::BTreeMap::new(),
            sent: (20.0, 20, false),
            sent_air: MAX_AIR,
            sent_fire: false,
            kills: 0,
            mobs_knocked: 0,
            mobs_fell: 0,
            hurt_by: std::collections::BTreeMap::new(),
            ender_spawns: 0,
            natural_spawns: false,
            blow_cause: None,
            hurt_by_mob: std::collections::BTreeMap::new(),
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
                loot: true,
                brain: Brain::Still,
                next_hit: 0,
                push: [0.0; 2],
                aloft: 0,
                vy: 0.0,
                fallen: 0.0,
            },
        );
        id
    }

    /// Place a mob that acts (`Brain`).
    pub fn summon_with(&mut self, kind: EntityKind, pos: [f64; 3], brain: Brain) -> i32 {
        let id = self.summon(kind, pos);
        if let Some(m) = self.survival.life.mobs.get_mut(&id) {
            m.brain = brain;
        }
        id
    }

    /// Make the spawner block at `pos` run: it spawns `kind` with `brain` by `BaseSpawner`'s
    /// rules.
    pub fn add_spawner(&mut self, pos: (i32, i32, i32), kind: EntityKind, brain: Brain) {
        self.survival.life.spawners.push(Spawner {
            pos,
            kind,
            brain,
            delay: SPAWNER_FIRST,
            spawned: 0,
        });
    }

    /// The spawners' tick (`BaseSpawner.serverTick`).
    fn tick_spawners(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        for i in 0..self.survival.life.spawners.len() {
            let Some(sp) = self.survival.life.spawners.get(i).copied() else {
                continue;
            };
            let centre = [
                f64::from(sp.pos.0) + 0.5,
                f64::from(sp.pos.1) + 0.5,
                f64::from(sp.pos.2) + 0.5,
            ];
            let near = (0..3)
                .map(|a| (self.pos[a] - centre[a]).powi(2))
                .sum::<f64>()
                < SPAWNER_PLAYER * SPAWNER_PLAYER;
            if !near || self.health <= 0.0 || kind_at(self, sp.pos) != Some(BlockKind::Spawner) {
                continue;
            }
            let mut delay = sp.delay - 1;
            let mut spawned = 0;
            if delay <= 0 {
                let crowd = self
                    .survival
                    .life
                    .mobs
                    .values()
                    .filter(|m| m.kind == sp.kind && m.dead_at.is_none())
                    .filter(|m| (0..3).all(|a| (m.pos[a] - centre[a]).abs() <= SPAWNER_RANGE + 0.5))
                    .count();
                if crowd < SPAWNER_NEARBY {
                    for _ in 0..SPAWNER_TRIES {
                        let mut tri = || self.survival.next_double() - self.survival.next_double();
                        let x = centre[0] + tri() * SPAWNER_RANGE;
                        let z = centre[2] + tri() * SPAWNER_RANGE;
                        let y =
                            f64::from(sp.pos.1) + (self.survival.next_double() * 3.0).floor() - 1.0;
                        // `noCollision` for a body 0.6 wide and 1.8 high, by its corners.
                        let free = [-0.3, 0.3].iter().all(|dx| {
                            [-0.3, 0.3].iter().all(|dz| {
                                [0.0, 0.9, 1.79].iter().all(|dy| {
                                    self.solid(
                                        (x + dx).floor() as i32,
                                        (y + dy).floor() as i32,
                                        (z + dz).floor() as i32,
                                    )
                                    .is_none()
                                })
                            })
                        });
                        if !free {
                            continue;
                        }
                        let id = self.summon_with(sp.kind, [x, y, z], sp.brain);
                        let mut uuid = [0u8; 16];
                        uuid[12..].copy_from_slice(&id.to_be_bytes());
                        out.push(wire(
                            State::Play,
                            &S2c::AddEntity {
                                id,
                                uuid,
                                kind: sp.kind.id(),
                                x,
                                y,
                                z,
                            },
                        )?);
                        spawned += 1;
                    }
                }
                // A cycle that spawned, or met a crowd, waits again; one that found no room
                // tries the next tick.
                if spawned > 0 || crowd >= SPAWNER_NEARBY {
                    delay = SPAWNER_MIN + (self.survival.next_double() * SPAWNER_SPAN) as i32;
                }
            }
            if let Some(s) = self.survival.life.spawners.get_mut(i) {
                s.delay = delay;
                s.spawned += spawned;
            }
        }
        Ok(out)
    }

    /// A mob's hit after armour (`CombatRules.getDamageAfterArmorAbsorb`, toughness 0).
    fn after_armor(&self, damage: f32) -> f32 {
        let armor = self.survival.life.armor;
        let cut = ARMOR_CAP.min((armor / 5.0).max(armor - 4.0 * damage / 8.0)) / 25.0;
        damage * (1.0 - cut.max(0.0))
    }

    /// A hit by a mob or its projectile from `from`: the damage after armour and, when it lands,
    /// the knock (`LivingEntity.knockback`, strength 0.4): the motion the player is sent this
    /// tick is 0.4 away from `from` on the plane, and 0.4 up when it stands on the ground.
    fn hit_from(
        &mut self,
        damage: f32,
        damage_type: &str,
        death_key: &str,
        from: [f64; 3],
    ) -> Result<Vec<Wire>, EncodeError> {
        let dealt = self.after_armor(damage);
        let out = self.hurt_player(dealt, damage_type, death_key)?;
        if !out.is_empty() {
            let (dx, dz) = (self.pos[0] - from[0], self.pos[2] - from[2]);
            let len = (dx * dx + dz * dz).sqrt().max(1e-9);
            // `LivingEntity.knockback`: on the ground `min(0.4, vy / 2 + 0.4)` of the server's own
            // vy (a tick of gravity: 0.3608); in the air the vy is kept (0 here: the tick's
            // motion packet then carries the server's own).
            let up = if self.survival.on_ground {
                (self.server_vy / 2.0 + KNOCKBACK).min(KNOCKBACK)
            } else {
                0.0
            };
            self.survival.life.knock = Some([dx / len * KNOCKBACK, up, dz / len * KNOCKBACK]);
        }
        Ok(out)
    }

    /// Whether nothing that collides lies between two points (sampled; a partial block counts
    /// as a whole one).
    pub fn sight(&self, a: [f64; 3], b: [f64; 3]) -> bool {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len = d.iter().map(|v| v * v).sum::<f64>().sqrt();
        let steps = (len / SAMPLE).ceil().max(1.0) as u32;
        (0..=steps).all(|i| {
            let t = f64::from(i) / f64::from(steps);
            let p = [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t];
            self.state_at(
                p[0].floor() as i32,
                p[1].floor() as i32,
                p[2].floor() as i32,
            )
            .is_none()
                || self
                    .solid(
                        p[0].floor() as i32,
                        p[1].floor() as i32,
                        p[2].floor() as i32,
                    )
                    .is_none()
        })
    }

    /// The blazes' tick (`Blaze$BlazeAttackGoal.tick`).
    fn tick_blazes(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let ids: Vec<i32> = self.survival.life.mobs.keys().copied().collect();
        for id in ids {
            let Some(m) = self.survival.life.mobs.get(&id).copied() else {
                continue;
            };
            let Brain::Blaze {
                attack_time,
                step,
                vy,
                offset,
                unseen,
                stroll,
            } = m.brain
            else {
                continue;
            };
            if m.dead_at.is_some() || self.health <= 0.0 || !self.ticking(m.pos) {
                continue;
            }
            let mut attack_time = attack_time - 1;
            let mut step = step;
            // Middle to middle (`getY(0.5)`): both bodies are 1.8 high.
            let from = [m.pos[0], m.pos[1] + 0.9, m.pos[2]];
            let to = [self.pos[0], self.pos[1] + 0.9, self.pos[2]];
            let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
            let dist2: f64 = d.iter().map(|v| v * v).sum();
            let seen = self.sight(
                [m.pos[0], m.pos[1] + 1.6, m.pos[2]],
                [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]],
            );
            if dist2 < 4.0 {
                if seen && attack_time <= 0 {
                    attack_time = BLAZE_MELEE_EVERY;
                    out.extend(self.blow_from(id, BLAZE_MELEE, m.pos, 0)?.1);
                }
            } else if dist2 < BLAZE_RANGE * BLAZE_RANGE && seen && attack_time <= 0 {
                step += 1;
                if step == 1 {
                    attack_time = BLAZE_CHARGE;
                } else if step <= 4 {
                    attack_time = BLAZE_GAP;
                } else {
                    attack_time = BLAZE_REST;
                    step = 0;
                }
                if step > 1 {
                    let spread = BLAZE_SPREAD * dist2.sqrt().sqrt() * 0.5;
                    let mut tri =
                        || spread * (self.survival.next_double() - self.survival.next_double());
                    let aim = [d[0] + tri(), d[1], d[2] + tri()];
                    let len = aim.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-9);
                    let fid = self.survival.next_id();
                    let pos = [from[0], from[1] + 0.5, from[2]];
                    self.survival.life.fireballs_fired += 1;
                    self.survival.life.fireballs.push(Fireball {
                        id: fid,
                        pos,
                        vel: [0.0; 3],
                        dir: aim.map(|v| v / len),
                        from: pos,
                        age: 0,
                    });
                    let mut uuid = [0u8; 16];
                    uuid[12..].copy_from_slice(&fid.to_be_bytes());
                    out.push(wire(
                        State::Play,
                        &S2c::AddEntity {
                            id: fid,
                            uuid,
                            kind: EntityKind::SmallFireball.id(),
                            x: pos[0],
                            y: pos[1],
                            z: pos[2],
                        },
                    )?);
                }
            }
            // Sideways, as the goals say.
            let unseen = if seen { 0 } else { unseen.saturating_add(1) };
            let targeted = unseen < BLAZE_MEMORY && dist2 < BLAZE_RANGE * BLAZE_RANGE;
            let mut stroll = stroll;
            let player = [self.pos[0], self.pos[2]];
            let goal = if (seen && dist2 < 4.0) || (!seen && targeted && unseen < BLAZE_CHASE) {
                stroll = None;
                Some(player)
            } else if targeted {
                stroll = None;
                None
            } else {
                if stroll.is_none() && self.survival.next_double() < 1.0 / BLAZE_STROLL_EVERY {
                    let x = m.pos[0] + (self.survival.next_double() * 2.0 - 1.0) * BLAZE_STROLL;
                    let z = m.pos[2] + (self.survival.next_double() * 2.0 - 1.0) * BLAZE_STROLL;
                    // A land position: a floor with two free cells over it within 7 up or down.
                    let land = (-7..=7).any(|dy| {
                        let y = m.pos[1].floor() as i32 + dy;
                        let c = |yy: i32| self.solid(x.floor() as i32, yy, z.floor() as i32);
                        c(y - 1).is_some() && c(y).is_none() && c(y + 1).is_none()
                    });
                    if land {
                        stroll = Some([x, z]);
                    }
                }
                stroll
            };
            let mut xz = [m.pos[0], m.pos[2]];
            if let Some(g) = goal {
                let (gx, gz) = (g[0] - xz[0], g[1] - xz[1]);
                let far = (gx * gx + gz * gz).sqrt();
                let grounded = self
                    .solid(
                        xz[0].floor() as i32,
                        (m.pos[1] - 0.05).floor() as i32,
                        xz[1].floor() as i32,
                    )
                    .is_some();
                let speed = if grounded { BLAZE_WALK } else { BLAZE_FLY };
                if far > 0.5 {
                    let next = [xz[0] + gx / far * speed, xz[1] + gz / far * speed];
                    let c = |dy: f64| {
                        self.solid(
                            next[0].floor() as i32,
                            (m.pos[1] + dy).floor() as i32,
                            next[1].floor() as i32,
                        )
                    };
                    if c(0.0).is_none() && c(1.79).is_none() {
                        xz = next;
                    } else {
                        stroll = None;
                    }
                } else {
                    stroll = None;
                }
            }
            // The flight, up and down.
            let offset = if self.ticks.is_multiple_of(BLAZE_OFFSET_EVERY) {
                // `triangle(0.5, 6.891)`: the mode plus the spread times (rand - rand).
                0.5 + BLAZE_OFFSET_SPREAD
                    * (self.survival.next_double() - self.survival.next_double())
            } else {
                offset
            };
            let eye = m.pos[1] + BLAZE_EYE;
            let mut vy = if self.pos[1] + physics::EYE_HEIGHT > eye + offset {
                vy + (BLAZE_RISE - vy) * BLAZE_RISE
            } else {
                vy
            };
            vy = (vy - physics::GRAVITY) * physics::VERTICAL_DRAG;
            if vy < 0.0 {
                vy *= BLAZE_FALL;
            }
            let to = [xz[0], m.pos[1] + vy, xz[1]];
            let at =
                |y: f64| self.solid(to[0].floor() as i32, y.floor() as i32, to[2].floor() as i32);
            let blocked = at(to[1]).is_some() || at(to[1] + 1.79).is_some();
            let pos = if blocked {
                [xz[0], m.pos[1], xz[1]]
            } else {
                to
            };
            if blocked {
                vy = 0.0;
            }
            if pos != m.pos {
                out.push(wire(
                    State::Play,
                    &S2c::EntityPositionSync {
                        id,
                        x: pos[0],
                        y: pos[1],
                        z: pos[2],
                    },
                )?);
            }
            if let Some(mob) = self.survival.life.mobs.get_mut(&id) {
                mob.pos = pos;
                mob.brain = Brain::Blaze {
                    attack_time,
                    step,
                    vy,
                    offset,
                    unseen,
                    stroll,
                };
            }
        }
        Ok(out)
    }

    /// The slot of a shield that is up and faces `source` (`LivingEntity.applyItemBlocking`:
    /// the item in use for its `block_delay_seconds`, and the damage's source position, the
    /// direct entity's, within `horizontal_blocking_angle` 90 of the look on the plane). Only
    /// for damage not in `#bypasses_shield` (a fireball's, a mob's blow).
    fn shield_stops(&self, source: [f64; 3]) -> Option<usize> {
        let life = &self.survival.life;
        let since = life.blocking_since?;
        if self.ticks < since + SHIELD_DELAY {
            return None;
        }
        let slot = life.blocking_slot;
        let held = self.survival.slots.get(slot).copied().flatten();
        if held.is_none_or(|(k, _)| k != ItemKind::Shield) {
            return None;
        }
        let yaw = f64::from(self.look.0).to_radians();
        let facing = [-yaw.sin(), yaw.cos()];
        let to = [source[0] - self.pos[0], source[2] - self.pos[2]];
        (facing[0] * to[0] + facing[1] * to[1] > 0.0).then_some(slot)
    }

    /// A hit the shield in `slot` stopped (`BlocksAttacks.hurtBlockingItem` and `onBlocked`):
    /// the block sound, and the shield's wear, sent as its `damage`; worn through, it breaks
    /// (`hurtAndBreak`: the slot emptied, the use stopped, the break sound).
    fn shield_takes(&mut self, slot: usize, damage: f32) -> Result<Vec<Wire>, EncodeError> {
        // `hurtServer` goes on with the blocked hit's 0: outside the invulnerable half it
        // starts a fresh 20 ticks with `lastHurt` 0, so the next hit in them lands whole.
        let life = &mut self.survival.life;
        if life.invulnerable <= INVULNERABLE_HALF {
            life.invulnerable = INVULNERABLE_TICKS;
            life.last_hurt = 0.0;
        }
        let mut out = vec![self.player_sound("item.shield.block")?];
        let wear = if damage < SHIELD_WEAR_THRESHOLD {
            0
        } else {
            (SHIELD_WEAR_BASE + SHIELD_WEAR_FACTOR * damage).floor() as i32
        };
        if wear == 0 {
            return Ok(out);
        }
        let max = data::generated::item_props::max_damage(ItemKind::Shield);
        let life = &mut self.survival.life;
        let worn = life.shield_wear.get(&slot).copied().unwrap_or(0) + wear;
        if worn >= max {
            life.shield_wear.remove(&slot);
            life.shields_broken += 1;
            life.blocking_since = None;
            if let Some(s) = self.survival.slots.get_mut(slot) {
                *s = None;
            }
            // The emptied slot goes out with the tick's `broadcast_changes`.
            out.push(self.player_sound("item.shield.break")?);
            return Ok(out);
        }
        life.shield_wear.insert(slot, worn);
        out.extend(self.wear_sent(slot)?);
        Ok(out)
    }

    /// A sound at the player's feet on the players' source, by its registry name.
    fn player_sound(&self, name: &str) -> Result<Wire, EncodeError> {
        let id = data::generated::registries::SOUND_EVENT
            .iter()
            .position(|n| *n == name)
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or(0);
        wire(
            State::Play,
            &S2c::Sound {
                sound: protocol::SoundRef::Registry(id),
                source: SOURCE_PLAYERS,
                pos: self.pos,
                volume: 1.0,
                pitch: 1.0,
            },
        )
    }

    /// A mob's blow at the player (`Mob.doHurtTarget`, damage type `mob_attack`): a raised
    /// shield facing it stops it whole, knocks the mob 0.5 back (`blockedByItem`) and, from an
    /// axe, is disabled for `disables` ticks (`BlocksAttacks.disable`: the use stopped, a
    /// cooldown); else the hit lands with its knock. Whether it landed.
    pub(crate) fn blow_from(
        &mut self,
        id: i32,
        damage: f32,
        from: [f64; 3],
        disables: u32,
    ) -> Result<(bool, Vec<Wire>), EncodeError> {
        if let Some(slot) = self.shield_stops(from) {
            self.survival.life.blows_blocked += 1;
            let mut out = self.shield_takes(slot, damage)?;
            let (px, pz) = (self.pos[0], self.pos[2]);
            if let Some(m) = self.survival.life.mobs.get_mut(&id) {
                m.knocked([m.pos[0] - px, m.pos[2] - pz], BLOCKED_KNOCK);
            }
            if disables > 0 {
                let life = &mut self.survival.life;
                life.blocking_since = None;
                life.shield_cooldown_until = self.ticks + u64::from(disables);
                out.push(self.player_sound("item.shield.break")?);
            }
            return Ok((false, out));
        }
        let out = self.hit_from(damage, "mob_attack", "death.attack.mob", from)?;
        Ok((!out.is_empty(), out))
    }

    /// The fireballs' tick: each flies, and ends on the player, on a block or of age.
    fn tick_fireballs(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let balls = std::mem::take(&mut self.survival.life.fireballs);
        let mut kept = Vec::new();
        let mut gone = Vec::new();
        for mut b in balls {
            let len = b.vel.iter().map(|v| v * v).sum::<f64>().sqrt();
            let steps = (len / SAMPLE).ceil().max(1.0) as u32;
            let body = physics::aabb(self.pos);
            let mut ended = false;
            let mut last = b.pos;
            for i in 1..=steps {
                let t = f64::from(i) / f64::from(steps);
                let p = [
                    b.pos[0] + b.vel[0] * t,
                    b.pos[1] + b.vel[1] * t,
                    b.pos[2] + b.vel[2] * t,
                ];
                let on_body = (0..3).all(|a| {
                    p[a] >= body[a] - PROJECTILE_MARGIN && p[a] <= body[a + 3] + PROJECTILE_MARGIN
                });
                let cell = (
                    p[0].floor() as i32,
                    p[1].floor() as i32,
                    p[2].floor() as i32,
                );
                if on_body && self.health > 0.0 {
                    // The source position is the fireball's own (`getSourcePosition`: the
                    // direct entity's), where it stood as this tick's move began.
                    if let Some(slot) = self.shield_stops(b.pos) {
                        self.survival.life.fireballs_blocked += 1;
                        out.extend(self.shield_takes(slot, FIREBALL_DAMAGE)?);
                    } else {
                        self.survival.life.fireballs_hit += 1;
                        let before = self.survival.life.fire_ticks;
                        if !self.survival.life.fire_resistance {
                            self.survival.life.fire_ticks =
                                self.survival.life.fire_ticks.max(FIREBALL_FIRE_TICKS);
                        }
                        let landed = self.hit_from(
                            FIREBALL_DAMAGE,
                            "fireball",
                            "death.attack.fireball",
                            b.from,
                        )?;
                        // A hit inside the invulnerable half with no excess deals nothing and
                        // the client hears of no damage: a race's "hit facts" count only these.
                        // `SmallFireball.onHitEntity` then puts the fire ticks back as they were.
                        if landed.is_empty() {
                            self.survival.life.fire_ticks = before;
                        } else {
                            self.survival.life.fireballs_landed += 1;
                        }
                        out.extend(landed);
                    }
                    ended = true;
                } else if self.state_at(cell.0, cell.1, cell.2).is_none() {
                    ended = true;
                } else if self.solid(cell.0, cell.1, cell.2).is_some() {
                    // `SmallFireball.onHitBlock`: fire in the cell before the block.
                    let before = (
                        last[0].floor() as i32,
                        last[1].floor() as i32,
                        last[2].floor() as i32,
                    );
                    let air = kind_at(self, before) == Some(BlockKind::Air);
                    let floor = self.solid(before.0, before.1 - 1, before.2) == Some(1.0);
                    if air && floor {
                        let fire = BlockKind::Fire.info().map_or(0, |i| i.default_state);
                        out.extend(self.setblock(before, fire)?);
                    }
                    ended = true;
                }
                if ended {
                    break;
                }
                last = p;
            }
            b.age += 1;
            if ended || b.age > FIREBALL_LIFE {
                gone.push(b.id);
                continue;
            }
            for a in 0..3 {
                b.pos[a] += b.vel[a];
                b.vel[a] = (b.vel[a] + b.dir[a] * FIREBALL_POWER) * FIREBALL_INERTIA;
            }
            out.push(wire(
                State::Play,
                &S2c::EntityPositionSync {
                    id: b.id,
                    x: b.pos[0],
                    y: b.pos[1],
                    z: b.pos[2],
                },
            )?);
            kept.push(b);
        }
        self.survival.life.fireballs = kept;
        if !gone.is_empty() {
            out.push(wire(State::Play, &S2c::RemoveEntities { ids: gone })?);
        }
        Ok(out)
    }

    /// Every fire block the world began with (a stage's, a captured world's) gets its own clock,
    /// as a fire's scheduled tick is kept with its chunk: such fires age and go out as the
    /// fireballs' do. Done once, at the first tick. How many were found.
    pub fn wake_fires(&mut self) -> usize {
        let fire = |s: u32| data::block_of_state(s) == Some(BlockKind::Fire);
        let mut cells = Vec::new();
        for (&(cx, cz), col) in &self.world.columns {
            let column = &col.column;
            for (si, section) in column.sections.iter().enumerate() {
                if !section.blocks.may_contain(fire) {
                    continue;
                }
                let base = column.min_y + 16 * i32::try_from(si).unwrap_or(0);
                for i in 0..4096usize {
                    if section.blocks.get(i).is_some_and(fire) {
                        let (lx, lz, ly) =
                            ((i & 15) as i32, ((i >> 4) & 15) as i32, (i >> 8) as i32);
                        cells.push((cx * 16 + lx, base + ly, cz * 16 + lz));
                    }
                }
            }
        }
        let mut found = 0;
        for p in cells {
            if self.survival.life.fires.iter().any(|f| f.pos == p) {
                continue;
            }
            self.fire_lit(p);
            found += 1;
        }
        found
    }

    /// `FireBlock.getFireTickDelay`: 30 + `nextInt(10)`.
    fn fire_delay(&mut self) -> u64 {
        FIRE_DELAY + (self.survival.next_double() * FIRE_DELAY_SPAN) as u64
    }

    /// `FireBlock.onPlace`: a fire set with updates (`set_and_update`) schedules its tick.
    pub(crate) fn fire_lit(&mut self, pos: (i32, i32, i32)) {
        let next = self.ticks + self.fire_delay();
        self.survival.life.fires.push(Fire { pos, age: 0, next });
    }

    /// The fireballs' fires (`FireBlock.tick`, each at its scheduled tick): gone if the cell
    /// is no longer fire; on an infiniburn floor (netherrack, magma: `#infiniburn_nether`)
    /// it burns on; else it ages by `nextInt(3) / 2` and, with no floor face under it or
    /// older than 3, goes out (the fortress holds nothing flammable, so `isValidFireLocation`
    /// is taken as false). The age is kept here, not sent in the block state.
    fn tick_fires(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        if !self.survival.life.fires_woken {
            self.survival.life.fires_woken = true;
            self.wake_fires();
        }
        let fires = std::mem::take(&mut self.survival.life.fires);
        let mut kept = Vec::with_capacity(fires.len());
        for mut f in fires {
            if kind_at(self, f.pos) != Some(BlockKind::Fire) {
                continue;
            }
            if self.ticks < f.next {
                kept.push(f);
                continue;
            }
            f.next = self.ticks + self.fire_delay();
            let (x, y, z) = f.pos;
            let below = kind_at(self, (x, y - 1, z));
            if matches!(below, Some(BlockKind::Netherrack | BlockKind::MagmaBlock)) {
                kept.push(f);
                continue;
            }
            let old = f.age;
            let grow = (self.survival.next_double() * 3.0).floor() as u8 / 2;
            f.age = (old + grow).min(15);
            let sturdy = self.solid(x, y - 1, z) == Some(1.0);
            if !sturdy || old > FIRE_OLD {
                let air = BlockKind::Air.info().map_or(0, |i| i.default_state);
                out.extend(self.setblock(f.pos, air)?);
                self.survival.life.fires_out += 1;
                continue;
            }
            kept.push(f);
        }
        // A fire placed while this ran (none can be) would be lost: append, not replace.
        kept.append(&mut self.survival.life.fires);
        self.survival.life.fires = kept;
        Ok(out)
    }

    /// The knocks the player's hits left: each knocked mob moves by its motion while its body
    /// fits where it goes (a walker also needs a floor there), and the motion is cut by the
    /// air's or the floor's drag. A blow into a wall stops.
    fn tick_pushes(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let ids: Vec<i32> = self.survival.life.mobs.keys().copied().collect();
        for id in ids {
            let Some(m) = self.survival.life.mobs.get(&id).copied() else {
                continue;
            };
            // A `Still` mob stands where it was put (a parity row's or a placed target): only
            // mobs that act are knocked.
            if m.dead_at.is_some()
                || m.push == [0.0; 2]
                || matches!(m.brain, Brain::Still)
                || !self.ticking(m.pos)
            {
                continue;
            }
            let to = [m.pos[0] + m.push[0], m.pos[1], m.pos[2] + m.push[1]];
            let cell = |p: [f64; 3], dy: f64| {
                self.solid(
                    p[0].floor() as i32,
                    (p[1] + dy).floor() as i32,
                    p[2].floor() as i32,
                )
            };
            let walker = matches!(m.brain, Brain::Melee(_));
            // A walker is carried over an edge as the game carries it (its fall is
            // `tick_falls`'); a blaze keeps to where its body fits.
            // An enderman is 2.9 high and 0.6 wide: its whole box must fit (issue 18: a hit never
            // pushes one into a burrow two high).
            let fits = if matches!(m.brain, Brain::Enderman(_)) {
                self.ender_fits(to)
            } else {
                cell(to, 0.0).is_none() && cell(to, 1.79).is_none()
            };
            let grounded = if walker {
                m.vy == 0.0 && cell(m.pos, -0.05).is_some()
            } else {
                m.aloft == 0 && cell(m.pos, -0.05).is_some()
            };
            let drag = if grounded { DRAG_GROUND } else { DRAG_AIR };
            let push = if fits {
                m.push.map(|v| v * drag)
            } else {
                [0.0; 2]
            };
            let push = if push[0].hypot(push[1]) < MOTION_FLOOR {
                [0.0; 2]
            } else {
                push
            };
            let pos = if fits { to } else { m.pos };
            if let Some(mob) = self.survival.life.mobs.get_mut(&id) {
                mob.pos = pos;
                mob.push = push;
                mob.aloft = m.aloft.saturating_sub(1);
            }
            if pos != m.pos {
                out.push(wire(
                    State::Play,
                    &S2c::EntityPositionSync {
                        id,
                        x: pos[0],
                        y: pos[1],
                        z: pos[2],
                    },
                )?);
            }
        }
        Ok(out)
    }

    /// The walkers' vertical motion (`LivingEntity.travel`): a knock's lift, then gravity 0.08
    /// and drag 0.98 a tick while nothing holds the feet; on landing, the fall's damage
    /// (`calculateFallDamage`: `ceil(fallen - safe_fall_distance 3)`). A walker knocked off a
    /// bridge falls to what is under it (a wither skeleton is fire immune: lava holds it as a
    /// floor here, out of the fight) and one that falls out of the world is gone.
    fn tick_falls(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let ids: Vec<i32> = self.survival.life.mobs.keys().copied().collect();
        for id in ids {
            let Some(m) = self.survival.life.mobs.get(&id).copied() else {
                continue;
            };
            if m.dead_at.is_some() || !matches!(m.brain, Brain::Melee(_)) || !self.ticking(m.pos) {
                continue;
            }
            let (x, z) = (m.pos[0].floor() as i32, m.pos[2].floor() as i32);
            let floor_at = |y: f64| {
                let c = y.floor() as i32;
                match self.state_at(x, c, z) {
                    None => None,
                    Some(s) => {
                        let lava = data::block_of_state(s) == Some(BlockKind::Lava);
                        self.solid(x, c, z)
                            .map(|top| f64::from(c) + top)
                            .or(lava.then_some(f64::from(c) + 1.0))
                    }
                }
            };
            let standing = m.vy == 0.0 && floor_at(m.pos[1] - 0.05).is_some();
            if standing {
                continue;
            }
            let mut y = m.pos[1] + m.vy;
            let mut vy = (m.vy - physics::GRAVITY) * physics::VERTICAL_DRAG;
            let mut fallen = m.fallen + (m.pos[1] - y).max(0.0);
            let mut landed = false;
            if m.vy > 0.0 && self.solid(x, (y + 1.79).floor() as i32, z).is_some() {
                y = m.pos[1];
                vy = 0.0;
            } else if m.vy <= 0.0
                && let Some(top) = floor_at(y)
                && top <= m.pos[1]
            {
                fallen -= top - y;
                y = top;
                vy = 0.0;
                landed = true;
            }
            let gone = self.state_at(x, y.floor() as i32, z).is_none() && y < m.pos[1];
            let hurt = if landed {
                (fallen - SAFE_FALL).ceil().max(0.0) as f32
            } else {
                0.0
            };
            let Some(mob) = self.survival.life.mobs.get_mut(&id) else {
                continue;
            };
            mob.pos[1] = y;
            mob.vy = vy;
            mob.fallen = if landed { 0.0 } else { fallen };
            if landed {
                mob.aloft = 0;
            }
            if hurt > 0.0 {
                mob.health = (mob.health - hurt).max(0.0);
                self.survival.life.mobs_fell += 1;
            }
            let dead = gone || mob.health <= 0.0;
            if dead {
                mob.dead_at = Some(self.ticks);
                out.push(wire(State::Play, &S2c::EntityEvent { id, status: 3 })?);
            } else {
                out.push(wire(
                    State::Play,
                    &S2c::EntityPositionSync {
                        id,
                        x: m.pos[0],
                        y,
                        z: m.pos[2],
                    },
                )?);
            }
        }
        Ok(out)
    }

    /// The melee mobs' tick: each comes straight at the player while the ground ahead holds it
    /// and the way is open, and hits in reach.
    fn tick_mobs(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let ids: Vec<i32> = self.survival.life.mobs.keys().copied().collect();
        for id in ids {
            let Some(m) = self.survival.life.mobs.get(&id).copied() else {
                continue;
            };
            let Brain::Melee(melee) = m.brain else {
                continue;
            };
            if m.dead_at.is_some() || !self.ticking(m.pos) {
                continue;
            }
            let (dx, dz) = (self.pos[0] - m.pos[0], self.pos[2] - m.pos[2]);
            let flat = (dx * dx + dz * dz).sqrt();
            let level = (self.pos[1] - m.pos[1]).abs() < 2.0;
            // Aloft from a knock it has no footing to walk with (its air control, 0.02 a
            // tick, is left out).
            if flat > melee.reach * 0.8 && m.aloft == 0 {
                let step = melee.speed.min(flat);
                let to = [
                    m.pos[0] + dx / flat * step,
                    m.pos[1],
                    m.pos[2] + dz / flat * step,
                ];
                let cell = |p: [f64; 3], dy: i32| {
                    self.solid(
                        p[0].floor() as i32,
                        p[1].floor() as i32 + dy,
                        p[2].floor() as i32,
                    )
                };
                if cell(to, -1).is_some() && cell(to, 0).is_none() && cell(to, 1).is_none() {
                    if let Some(mob) = self.survival.life.mobs.get_mut(&id) {
                        mob.pos = to;
                    }
                    out.push(wire(
                        State::Play,
                        &S2c::EntityPositionSync {
                            id,
                            x: to[0],
                            y: to[1],
                            z: to[2],
                        },
                    )?);
                }
            }
            if flat <= melee.reach && level && self.ticks >= m.next_hit && self.health > 0.0 {
                if let Some(mob) = self.survival.life.mobs.get_mut(&id) {
                    mob.next_hit = self.ticks + MELEE_EVERY;
                }
                let (landed, hit) =
                    self.blow_from(id, melee.damage, m.pos, melee.disables_shield)?;
                out.extend(hit);
                // `WitherSkeleton.doHurtTarget`: the effect only when the blow landed.
                if landed {
                    let life = &mut self.survival.life;
                    life.wither_ticks = life.wither_ticks.max(melee.wither_ticks);
                }
            }
        }
        Ok(out)
    }

    /// The mobs' `add_entity`, for the join.
    pub(crate) fn mobs_added(&self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        for (id, m) in self
            .survival
            .life
            .mobs
            .iter()
            .filter(|(_, m)| m.dead_at.is_none())
        {
            let mut uuid = [0u8; 16];
            uuid[12..].copy_from_slice(&id.to_be_bytes());
            out.push(wire(
                State::Play,
                &S2c::AddEntity {
                    id: *id,
                    uuid,
                    kind: m.kind.id(),
                    x: m.pos[0],
                    y: m.pos[1],
                    z: m.pos[2],
                },
            )?);
            // An enderman a test placed already angry (issue 18: a frozen angry one holding the
            // bot) shows its creepy flag from the join, as a server sends its data with it.
            if let Brain::Enderman(e) = m.brain
                && e.angry
            {
                out.push(wire(
                    State::Play,
                    &S2c::SetEntityData {
                        id: *id,
                        entries: vec![(
                            crate::enderman::META_CREEPY,
                            protocol::metadata::MetaValue::Bool(true),
                        )],
                    },
                )?);
            }
        }
        Ok(out)
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
        if self.survival.life.fire_resistance
            && matches!(
                damage_type,
                "lava" | "in_fire" | "on_fire" | "hot_floor" | "fireball"
            )
        {
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
        *life.hurt_by.entry(damage_type.to_owned()).or_insert(0.0) += dealt;
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
        // `lava`, `in_fire` and `hot_floor` are not in `#bypasses_armor` (the 26.1.2 jar's
        // damage type tags; `on_fire`, `wither`, `starve`, `drown`, `fall` are): armour cuts them
        // as it cuts a blow.
        if self.box_touches(&[BlockKind::Lava]) {
            self.survival.life.fire_ticks = self.survival.life.fire_ticks.max(LAVA_FIRE_TICKS);
            let dealt = self.after_armor(4.0);
            out.extend(self.hurt_player(dealt, "lava", "death.attack.lava")?);
        } else if self.box_touches(&[BlockKind::Fire, BlockKind::SoulFire]) {
            self.survival.life.fire_ticks = self.survival.life.fire_ticks.max(FIRE_BLOCK_TICKS);
            let dealt = self.after_armor(1.0);
            out.extend(self.hurt_player(dealt, "in_fire", "death.attack.inFire")?);
        } else if self.survival.life.fire_ticks > 0 {
            if self.survival.life.fire_ticks % 20 == 0 {
                out.extend(self.hurt_player(1.0, "on_fire", "death.attack.onFire")?);
            }
            self.survival.life.fire_ticks -= 1;
        }
        // Magma under the feet.
        let on = (
            self.pos[0].floor() as i32,
            (self.pos[1] - ON_POS_BELOW).floor() as i32,
            self.pos[2].floor() as i32,
        );
        if self.survival.on_ground
            && !self.survival.life.sneaking
            && kind_at(self, on) == Some(BlockKind::MagmaBlock)
        {
            let dealt = self.after_armor(MAGMA_DAMAGE);
            out.extend(self.hurt_player(dealt, "hot_floor", "death.attack.hotFloor")?);
        }
        // The wither effect, then the mobs that act.
        if self.survival.life.wither_ticks > 0 {
            if self.survival.life.wither_ticks.is_multiple_of(WITHER_EVERY) {
                out.extend(self.hurt_player(1.0, "wither", "death.attack.wither")?);
            }
            self.survival.life.wither_ticks -= 1;
        }
        out.extend(self.tick_fires()?);
        out.extend(self.tick_pushes()?);
        out.extend(self.tick_falls()?);
        out.extend(self.tick_mobs()?);
        out.extend(self.tick_spawners()?);
        out.extend(self.tick_blazes()?);
        out.extend(self.tick_fireballs()?);
        out.extend(self.tick_endermen()?);
        out.extend(self.tick_ender_spawns()?);
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
        // A shield in either hand goes up (the offhand is menu slot 45).
        // Not while an axe's blow has it cooling down (`ItemCooldowns`: the use is refused).
        let shield = |s: Option<(ItemKind, i32)>| s.is_some_and(|(k, _)| k == ItemKind::Shield);
        let slot = if shield(self.survival.held_stack()) {
            Some(self.survival.held_slot_index())
        } else if shield(self.survival.slots.get(45).copied().flatten()) {
            Some(45)
        } else {
            None
        };
        if let Some(slot) = slot
            && self.ticks >= self.survival.life.shield_cooldown_until
            && self.survival.life.blocking_since.is_none()
        {
            self.survival.life.blocking_since = Some(self.ticks);
            self.survival.life.blocking_slot = slot;
        }
        // A bucket is used along the look (`BucketItem.use`, issue 14a).
        if let Some(used) = self.use_bucket() {
            let mut out = used?;
            out.push(wire(State::Play, &S2c::BlockChangedAck { sequence })?);
            return Ok(out);
        }
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
        self.survival.life.blocking_since = None;
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
        // The distance from the eye to the mob's box (0.6 wide, 1.95 tall for a zombie-sized one;
        // an enderman's 2.9).
        let (hw, h) = if m.kind == EntityKind::Enderman {
            (crate::enderman::HALF_WIDTH, crate::enderman::HEIGHT)
        } else {
            (0.3, 1.95)
        };
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
            // A fresh hit knocks the mob 0.4 away from the player on the plane, half its
            // motion kept (`hurtServer` → `knockback(0.4, …)`; a hit inside the invulnerable
            // half knocks nothing). Blazes and wither skeletons resist none.
            m.knocked([m.pos[0] - self.pos[0], m.pos[2] - self.pos[2]], KNOCKBACK);
            life.mobs_knocked += 1;
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
        let (kind, at, loot) = (m.kind, m.pos, m.loot);
        out.extend(self.ender_hurt_by_player(entity)?);
        let life = &mut self.survival.life;
        let Some(m) = life.mobs.get_mut(&entity) else {
            return Ok(out);
        };
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
            if loot {
                out.extend(self.drop_loot(kind, at)?);
            }
        }
        Ok(out)
    }

    /// `dropAllDeathLoot`: each unconditioned drop's count rolled in its range
    /// (`UniformGenerator.getInt`), spawned at the mob's feet as `spawnAtLocation` does (a
    /// random sideways push of up to 0.1, 0.2 up, the default pickup delay of 10 ticks).
    fn drop_loot(&mut self, kind: EntityKind, at: [f64; 3]) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        // The generated table holds the unconditioned pools. A pool that asks for
        // `killed_by_player` is added here, since every kill in the model is the player's: a
        // blaze's rod, 0 or 1 (`loot_table/entities/blaze.json` in the 26.1.2 jar).
        let by_player: &[(EntityKind, ItemKind, i64, i64)] =
            &[(EntityKind::Blaze, ItemKind::BlazeRod, 0, 1)];
        let drops: Vec<(ItemKind, i64, i64)> = data::generated::loot::mob_drops_of(kind)
            .iter()
            .map(|d| (d.item, d.min, d.max))
            .chain(
                by_player
                    .iter()
                    .filter(|(k, ..)| *k == kind)
                    .map(|(_, item, min, max)| (*item, *min, *max)),
            )
            .collect();
        for (item, min, max) in drops {
            let span = (max - min + 1).max(1) as f64;
            let n = min + (self.survival.next_double() * span).floor() as i64;
            let Ok(count) = i32::try_from(n) else {
                continue;
            };
            if count <= 0 {
                continue;
            }
            let vx = self.survival.next_double() * 0.2 - 0.1;
            let vz = self.survival.next_double() * 0.2 - 0.1;
            out.extend(self.spawn_item((item, count), at, [vx, 0.2, vz], 10)?);
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
