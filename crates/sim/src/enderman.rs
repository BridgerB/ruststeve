//! Issue 17: endermen, each rule read from the 26.1.2 server jar with `javap` (probe
//! 45472254333: `EnderMan`, its goals, `LivingEntity.isLookingAtMe`, `DistanceManager`,
//! `ServerLevel`, `NaturalSpawner`; docs/problems/endermen-model.md).
//!
//! - **Frozen beyond the simulation distance** (`ServerLevel.tick`'s entity loop returns for a
//!   non-player entity whose chunk is not `inEntityTickingRange`, and `ChunkMap` sends its changes
//!   only there): no movement, no goal, no teleport, no position update. The player's simulation
//!   ticket is level `31 - simulation_distance` (`getPlayerTicketLevel`), entity ticking is level
//!   31 or less: a Chebyshev chunk distance up to the simulation distance (`Sim::ticking`).
//! - **The stare** (`EnderMan.isBeingStaredBy`): no carved pumpkin on the head
//!   (`#gaze_disguise_equipment`), and `isLookingAtMe(player, 0.025, true, false, eyeY)`: the
//!   player's view vector against the unit vector from the player's eye to the enderman's eye,
//!   `dot > 1 - 0.025 / distance`, with a collider line of sight from eye to eye. The eye alone:
//!   no other height counts.
//! - **Anger** (`EndermanLookForPlayerGoal`, target selector priority 1): the goal selectors run
//!   whole on even ticks of the mob's age (`Mob.serverAiStep`); a stare within the follow range
//!   64 starts the goal (metadata 18, stared at, goes true), `aggroTime` = `adjustedTickDelay(5)`
//!   = 3 goal ticks counted down while the stare holds (a look away cancels it), then the player
//!   is the target: metadata 17 (creepy) true and the speed +0.15 (`setTarget`). A hit from the
//!   player angers it too (`HurtByTargetGoal`). Anger is kept for the run (mc-mob-facts).
//! - **Teleports**: targeted and stared at within 4 (distance squared under 16), `teleport()`: a
//!   place drawn in a box 64 wide and 64 high round it; farther than 16 (over 256), after
//!   `adjustedTickDelay(30)` = 15 goal ticks (30 server ticks) `teleportTowards` the target: 16
//!   back along the line from the target, spread 8 sideways and 16 in height. A landing goes
//!   down to the first block that blocks motion, not water, and needs the body free of blocks
//!   and liquid (`Entity.randomTeleport`); one try per call. A hit by a living entity never
//!   teleports it (`hurtServer`).
//! - **Moving**: stroll (`WaterAvoidingRandomStrollGoal(1.0, 0)`: one start in 120 goal checks,
//!   a land place within 10 across and 7 up or down) at 0.3 speed; targeted, at 0.45 along a
//!   path (`MeleeAttackGoal`: a search over walk nodes for a mob 2.9 high to the node nearest the
//!   player, a partial path when the player cannot be reached; issue 18);
//!   frozen in place while targeted and stared at within 16 (`EndermanFreezeWhenLookedAt`). It
//!   hits for 7 (`attack_damage`) every 20 ticks when its box grown 0.83 sideways meets the
//!   player's and it sees the player eye to eye (`canPerformAttack`, issue 18); the damage event
//!   names it. A knock moves it only where its whole box fits (issue 18: never into a burrow).
//! - **Natural spawning** (`NaturalSpawner.spawnCategoryForPosition`): each tick, for each chunk
//!   in entity-ticking range with its middle within 128 of the player and the monster cap (70)
//!   not met: a place in the chunk at a height up to its top block + 1, not inside a full
//!   block; three packs of `ceil(4 rand)` tries, each moved `nextInt(6) - nextInt(6)` on x and z;
//!   a try spawns where the player is more than 24 away and within 128, the biome is
//!   `warped_forest` or `soul_sand_valley` (a pack's kind drawn by weight from the biome's monster
//!   list, its group size then the pack's tries; the model spawns endermen only), the body (0.6 by 2.9)
//!   is free of blocks and liquid over a floor that collides, and the spawn cost allows it
//!   (the biome's charge and energy budget: the sum of charge² / distance to the endermen
//!   already there at most the budget; the call stops at 4 spawned. Skeletons and ghasts carry
//!   charges in real but are not spawned here, so the model's endermen meet less resistance).
//!
//! Left out: light (no light in the model: every place is dark enough), the block carried and
//! taken (`EndermanTakeBlockGoal`: the Nether has nothing it picks up but crimson and warped
//! nylium, never sent here), water and rain hurting it, the random despawn (1 in 800 a tick
//! past 32 blocks after 30 s idle; the 128-block despawn is in), the daylight teleport (the
//! overworld at day), sounds, the head's rotation, the pathfinder's costs (every node costs the same here).

use data::{BlockKind, EntityKind, ItemKind};
use protocol::metadata::MetaValue;
use protocol::{EncodeError, S2c, State};

use crate::life::{Brain, Mob};
use crate::{Sim, Wire, wire};

/// `EntityType.ENDERMAN`: 0.6 wide, 2.9 high, the eye at 2.55.
pub const HALF_WIDTH: f64 = 0.3;
pub const HEIGHT: f64 = 2.9;
pub const EYE: f64 = 2.55;
/// Entity-data indexes (Entity 0 to 7, LivingEntity 8 to 14, Mob 15, then `EnderMan`'s own in
/// `defineSynchedData` order): the carried block, creepy (has a target), stared at.
pub const META_CARRIED: u8 = 16;
pub const META_CREEPY: u8 = 17;
pub const META_STARED_AT: u8 = 18;
/// `isBeingStaredBy`: `isLookingAtMe(player, 0.025, scaleByDistance true, …)`.
const STARE_TOLERANCE: f64 = 0.025;
/// The player's eye standing and crouching.
const PLAYER_EYE_CROUCHING: f64 = 1.27;
/// `createAttributes`: follow range 64, attack 7, movement speed 0.3; `SPEED_MODIFIER_ATTACKING`
/// adds 0.15 while it has a target.
const FOLLOW_RANGE: f64 = 64.0;
pub const ATTACK: f32 = 7.0;
/// Ground speeds in blocks a tick, as the model's wither skeleton's are derived (life.rs): the
/// speed squared over 0.454; 0.3 strolling, 0.45 chasing.
const STROLL_SPEED: f64 = 0.3 * 0.3 / 0.454;
const CHASE_SPEED: f64 = 0.45 * 0.45 / 0.454;
/// `aggroTime = adjustedTickDelay(5)`, `teleportTime >= adjustedTickDelay(30)`: goal ticks, one
/// each 2 server ticks (`reducedTickDelay`: ceil(n / 2)).
const AGGRO_GOAL_TICKS: u32 = 3;
const TELEPORT_GOAL_TICKS: u32 = 15;
/// Distances squared: stared within 4 teleports away, farther than 16 teleports toward, and the
/// freeze holds within 16.
const NEAR_SQ: f64 = 16.0;
const FAR_SQ: f64 = 256.0;
/// `teleport()`'s box (64 across, `nextInt(64) - 32` up and down) and `teleportTowards`'s
/// (16 back along the line, 8 spread, `nextInt(16) - 8` in height).
const TELEPORT_BOX: f64 = 64.0;
const TOWARD_BACK: f64 = 16.0;
const TOWARD_SPREAD: f64 = 8.0;
/// `MeleeAttackGoal`: the attack box is the mob's grown 0.83 sideways (`DEFAULT_ATTACK_REACH`);
/// a hit each 20 ticks.
const ATTACK_REACH: f64 = 0.83;
const ATTACK_EVERY: u64 = 20;
/// `RandomStrollGoal`: one start in 120 checks; `LandRandomPos.getPos(mob, 10, 7)`.
const STROLL_EVERY: f64 = 120.0;
const STROLL_RANGE: f64 = 10.0;
const STROLL_DY: i32 = 7;
/// `STEP_HEIGHT` 1.
const STEP: i32 = 1;
/// The chase's path search (issue 18): `PathNavigation.createPath`'s most nodes visited, the follow
/// range 64 × 16; the furthest drop a walk node takes (`Mob.getMaxFallDistance` 3); the ticks
/// a path is kept before it is found again (`MeleeAttackGoal` asks every 4 to 11).
const PATH_NODES: usize = 1024;
const PATH_DROP: i32 = 3;
const PATH_EVERY: u64 = 10;
/// `NaturalSpawner`: `MIN_SPAWN_DISTANCE` 24 (576 squared), `SPAWN_DISTANCE_BLOCK` 128, three
/// packs, `ceil(nextFloat() × 4)` tries, a step of `nextInt(6) - nextInt(6)`, the monster
/// category's 70 (`MobCategory.MONSTER`, over the 289 chunks one player's 8 counts), and
/// `Mob.getMaxSpawnClusterSize` 4.
const SPAWN_MIN_SQ: f64 = 576.0;
const SPAWN_MAX_SQ: f64 = 16384.0;
const PACKS: u32 = 3;
const PACK_TRIES: f64 = 4.0;
const PACK_STEP: f64 = 6.0;
const MONSTER_CAP: usize = 70;
const CLUSTER: u32 = 4;

/// A Nether biome's `spawners.monster` (kind, weight, min, max), from
/// `data/minecraft/worldgen/biome/<biome>.json` in the 26.1.2 jar (probes 45472254333 and
/// 45475308661). Kinds the model does not spawn still take their share of the draws.
fn monsters_of(biome: Option<&str>) -> &'static [(EntityKind, u32, u32, u32)] {
    match biome {
        Some("warped_forest") => &[(EntityKind::Enderman, 1, 4, 4)],
        Some("nether_wastes") => &[
            (EntityKind::Ghast, 50, 4, 4),
            (EntityKind::ZombifiedPiglin, 100, 4, 4),
            (EntityKind::MagmaCube, 2, 4, 4),
            (EntityKind::Enderman, 1, 4, 4),
            (EntityKind::Piglin, 15, 4, 4),
        ],
        Some("soul_sand_valley") => &[
            (EntityKind::Skeleton, 20, 5, 5),
            (EntityKind::Ghast, 50, 4, 4),
            (EntityKind::Enderman, 1, 4, 4),
        ],
        _ => &[],
    }
}

/// A biome's `spawn_costs` for a kind: (charge, energy budget).
fn spawn_cost(biome: Option<&str>, kind: EntityKind) -> Option<(f64, f64)> {
    match (biome, kind) {
        (Some("warped_forest"), EntityKind::Enderman) => Some((1.0, 0.12)),
        (Some("soul_sand_valley"), EntityKind::Enderman) => Some((0.7, 0.15)),
        _ => None,
    }
}

/// An enderman's own state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ender {
    /// The tick it was placed: the goal selectors run whole on even ticks of its age.
    pub born: u64,
    /// The player is its target (metadata 17).
    pub angry: bool,
    /// Metadata 18 went true (the look goal started once).
    pub stared_at: bool,
    /// The look goal's `aggroTime` while a stare is pending.
    pub pending: Option<u32>,
    /// The look goal's `teleportTime`.
    pub teleport_time: u32,
    /// Where a stroll is taking it (x, z).
    pub stroll: Option<[f64; 2]>,
    /// Its upward speed while it falls.
    pub vy: f64,
    /// Spawned by the world (despawns past 128 blocks), not placed by a test.
    pub natural: bool,
    /// Counts for tests: teleports made, ticks it ticked.
    pub teleports: u32,
    pub ticked: u64,
    /// The chase's path (issue 18): the next cell on it, the player's cell it was found for and
    /// the tick it was found.
    pub path_next: Option<[i32; 3]>,
    pub path_for: Option<[i32; 3]>,
    pub path_at: u64,
}

impl Ender {
    pub fn new(born: u64) -> Self {
        Self {
            born,
            angry: false,
            stared_at: false,
            pending: None,
            teleport_time: 0,
            stroll: None,
            vy: 0.0,
            natural: false,
            teleports: 0,
            ticked: 0,
            path_next: None,
            path_for: None,
            path_at: 0,
        }
    }
}

/// The squared distance between two feet positions (`Entity.distanceToSqr`).
fn dist_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

impl Sim {
    /// Place an enderman (a test's hook; a natural spawn uses the same): 40 health, no target.
    pub fn summon_enderman(&mut self, pos: [f64; 3]) -> i32 {
        let ender = Ender::new(self.ticks);
        self.summon_with(EntityKind::Enderman, pos, Brain::Enderman(ender))
    }

    /// Natural spawning on (issue 17): endermen in the Nether's biomes as the jar's
    /// `NaturalSpawner` places them. Off by default, so the closed-loop worlds of earlier issues
    /// (all `nether_wastes`, whose packs draw an enderman 1 in 168) keep their mobs.
    pub fn with_natural_spawns(mut self) -> Self {
        self.survival.life.natural_spawns = true;
        self
    }

    /// An enderman's state, for a test.
    pub fn ender(&self, id: i32) -> Option<Ender> {
        match self.survival.life.mobs.get(&id)?.brain {
            Brain::Enderman(e) => Some(e),
            Brain::Still | Brain::Melee(_) | Brain::Blaze { .. } => None,
        }
    }

    /// The player's eye as the server holds it (crouching lowers it).
    fn player_eye(&self) -> [f64; 3] {
        let h = if self.survival.life.sneaking {
            PLAYER_EYE_CROUCHING
        } else {
            physics::EYE_HEIGHT
        };
        [self.pos[0], self.pos[1] + h, self.pos[2]]
    }

    /// `EnderMan.isBeingStaredBy(player)` for an enderman whose feet are at `at`.
    pub fn stares_at(&self, at: [f64; 3]) -> bool {
        let head = self.survival.slots.get(5).copied().flatten();
        if head.is_some_and(|(k, _)| k == ItemKind::CarvedPumpkin) {
            return false;
        }
        let (yaw, pitch) = (
            f64::from(self.look.0).to_radians(),
            f64::from(self.look.1).to_radians(),
        );
        let view = [
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            yaw.cos() * pitch.cos(),
        ];
        let eye = self.player_eye();
        let target = [at[0], at[1] + EYE, at[2]];
        let d = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
        let len = d.iter().map(|v| v * v).sum::<f64>().sqrt();
        if len < 1e-9 {
            return false;
        }
        let dot = (0..3).map(|i| view[i] * d[i] / len).sum::<f64>();
        dot > 1.0 - STARE_TOLERANCE / len && self.sight(eye, target)
    }

    /// Whether the body of an enderman at `at` fits: no block it collides with and no liquid.
    pub(crate) fn ender_fits(&self, at: [f64; 3]) -> bool {
        let lo = at[1].floor() as i32;
        let hi = (at[1] + HEIGHT - 1e-7).floor() as i32;
        let xs = [
            (at[0] - HALF_WIDTH).floor() as i32,
            (at[0] + HALF_WIDTH - 1e-7).floor() as i32,
        ];
        let zs = [
            (at[2] - HALF_WIDTH).floor() as i32,
            (at[2] + HALF_WIDTH - 1e-7).floor() as i32,
        ];
        (lo..=hi).all(|y| {
            xs.iter().all(|&x| {
                zs.iter().all(|&z| {
                    let liquid = self
                        .state_at(x, y, z)
                        .and_then(data::block_of_state)
                        .is_some_and(|k| matches!(k, BlockKind::Water | BlockKind::Lava));
                    let solid = self
                        .solid(x, y, z)
                        .is_some_and(|top| f64::from(y) + top > at[1] + 1e-7);
                    self.state_at(x, y, z).is_some() && !liquid && !solid
                })
            })
        })
    }

    /// Whether something that collides holds feet at `at`.
    fn floored(&self, at: [f64; 3]) -> bool {
        self.solid(
            at[0].floor() as i32,
            (at[1] - 0.05).floor() as i32,
            at[2].floor() as i32,
        )
        .is_some()
    }

    /// `EnderMan.teleport(x, y, z)` then `Entity.randomTeleport`: down to the first block that
    /// blocks motion; that block not water; the body free there. The new feet, or `None`.
    fn ender_landing(&self, x: f64, y: f64, z: f64) -> Option<[f64; 3]> {
        let (bx, bz) = (x.floor() as i32, z.floor() as i32);
        let min_y = self.world.columns.get(&(bx >> 4, bz >> 4))?.column.min_y;
        let mut cell = y.floor() as i32;
        let mut feet = y;
        let blocks = |c: i32| self.solid(bx, c, bz).is_some();
        // A place outside the loaded columns is no landing (`hasChunkAt`).
        self.state_at(bx, cell, bz)?;
        while cell > min_y && !blocks(cell) {
            cell -= 1;
        }
        if !blocks(cell) {
            return None;
        }
        let water = self
            .state_at(bx, cell, bz)
            .and_then(data::block_of_state)
            .is_some_and(|k| k == BlockKind::Water);
        if water {
            return None;
        }
        // `randomTeleport` steps the feet down one a time from y until the block under them
        // blocks motion.
        let mut c = y.floor() as i32;
        while c > min_y && !blocks(c - 1) {
            feet -= 1.0;
            c -= 1;
        }
        let at = [x, feet, z];
        self.ender_fits(at).then_some(at)
    }

    /// A teleport of mob `id` to a landing, sent; whether it went.
    fn ender_teleport(
        &mut self,
        id: i32,
        x: f64,
        y: f64,
        z: f64,
    ) -> Result<Option<Wire>, EncodeError> {
        let Some(to) = self.ender_landing(x, y, z) else {
            return Ok(None);
        };
        if let Some(m) = self.survival.life.mobs.get_mut(&id) {
            m.pos = to;
        }
        Ok(Some(wire(
            State::Play,
            &S2c::EntityPositionSync {
                id,
                x: to[0],
                y: to[1],
                z: to[2],
            },
        )?))
    }

    /// `teleport()`: a random place round it.
    fn teleport_random(&mut self, id: i32, at: [f64; 3]) -> Result<Option<Wire>, EncodeError> {
        let x = at[0] + (self.survival.next_double() - 0.5) * TELEPORT_BOX;
        let y = at[1] + ((self.survival.next_double() * TELEPORT_BOX).floor() - TELEPORT_BOX / 2.0);
        let z = at[2] + (self.survival.next_double() - 0.5) * TELEPORT_BOX;
        self.ender_teleport(id, x, y, z)
    }

    /// `teleportTowards(player)`.
    fn teleport_toward(&mut self, id: i32, at: [f64; 3]) -> Result<Option<Wire>, EncodeError> {
        let eye = self.player_eye();
        let v = [
            at[0] - eye[0],
            at[1] + HEIGHT / 2.0 - eye[1],
            at[2] - eye[2],
        ];
        let len = v.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-9);
        let v = v.map(|c| c / len);
        let x = at[0] + (self.survival.next_double() - 0.5) * TOWARD_SPREAD - v[0] * TOWARD_BACK;
        let y = at[1] + ((self.survival.next_double() * 16.0).floor() - 8.0) - v[1] * TOWARD_BACK;
        let z = at[2] + (self.survival.next_double() - 0.5) * TOWARD_SPREAD - v[2] * TOWARD_BACK;
        self.ender_teleport(id, x, y, z)
    }

    /// One step along the ground toward (gx, gz) at `speed`, stepping up one block or down to a
    /// floor under, the body fitting; `None` where it cannot go.
    fn ender_step(&self, at: [f64; 3], goal: [f64; 2], speed: f64) -> Option<[f64; 3]> {
        // The goal is (x, z); `at` is (x, y, z). Issue 17's model read z from at[1], the height:
        // every stroll and chase ran along z toward 40 blocks off (issue 18's parity rows found it).
        let (dx, dz) = (goal[0] - at[0], goal[1] - at[2]);
        let far = dx.hypot(dz);
        if far < 0.1 {
            return None;
        }
        let s = speed.min(far);
        let next = [at[0] + dx / far * s, at[1], at[2] + dz / far * s];
        (0..=STEP)
            .map(|up| [next[0], at[1].floor() + f64::from(up), next[2]])
            .chain(std::iter::once([next[0], at[1], next[2]]))
            .find(|p| self.ender_fits(*p) && self.floored(*p))
            .or_else(|| {
                // Off an edge: it walks on and falls.
                let p = [next[0], at[1], next[2]];
                self.ender_fits(p).then_some(p)
            })
    }

    /// Whether an enderman stands in cell `c`: its body fits at the cell's middle and a floor
    /// holds it (`WalkNodeEvaluator`'s walkable node for a mob 2.9 high: three cells clear).
    fn ender_stands(&self, c: [i32; 3]) -> bool {
        let at = [
            f64::from(c[0]) + 0.5,
            f64::from(c[1]),
            f64::from(c[2]) + 0.5,
        ];
        self.ender_fits(at) && self.floored(at)
    }

    /// The node a walk from standing cell `c` reaches moving one cell (dx, dz): the same level,
    /// one up (the body fitting over `c` too), or down as far as `PATH_DROP`.
    fn ender_neighbour(&self, c: [i32; 3], dx: i32, dz: i32) -> Option<[i32; 3]> {
        let (x, z) = (c[0] + dx, c[2] + dz);
        let mid = |x: i32, y: i32, z: i32| [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5];
        if self.ender_stands([x, c[1], z]) {
            return Some([x, c[1], z]);
        }
        if self.ender_stands([x, c[1] + STEP, z]) && self.ender_fits(mid(c[0], c[1] + STEP, c[2])) {
            return Some([x, c[1] + STEP, z]);
        }
        if !self.ender_fits(mid(x, c[1], z)) {
            return None;
        }
        (1..=PATH_DROP)
            .map(|d| [x, c[1] - d, z])
            .find(|n| self.ender_stands(*n))
    }

    /// `PathFinder.findPath` toward the player's cell, as the chase's `MeleeAttackGoal` asks
    /// for it: a search over walk nodes from the mob's cell, at most `PATH_NODES` of them, and
    /// the path to the node nearest the target (a partial path when the target cannot be
    /// reached: a player in a burrow too low for the mob). Ties go to the node found first.
    /// The next cell on that path, or `None` when the mob already stands on the nearest node.
    fn ender_path(&self, from: [f64; 3], target: [i32; 3]) -> Option<[i32; 3]> {
        let start = [
            from[0].floor() as i32,
            (from[1] + 1e-7).floor() as i32,
            from[2].floor() as i32,
        ];
        let d2 = |c: [i32; 3]| -> i64 {
            (0..3)
                .map(|i| i64::from(c[i] - target[i]).pow(2))
                .sum::<i64>()
        };
        let mut came: std::collections::BTreeMap<[i32; 3], [i32; 3]> =
            std::collections::BTreeMap::new();
        let mut queue = std::collections::VecDeque::from([start]);
        came.insert(start, start);
        let mut best = (d2(start), start);
        while let Some(c) = queue.pop_front() {
            if d2(c) < best.0 {
                best = (d2(c), c);
            }
            if came.len() >= PATH_NODES {
                continue;
            }
            // The four sides, then the corners (a corner only past two open sides).
            let sides = [(1, 0), (-1, 0), (0, 1), (0, -1)];
            let open: Vec<Option<[i32; 3]>> = sides
                .iter()
                .map(|&(dx, dz)| self.ender_neighbour(c, dx, dz))
                .collect();
            let level = |i: usize| open.get(i).copied().flatten().is_some_and(|n| n[1] == c[1]);
            // Side indexes: +x 0, -x 1, +z 2, -z 3.
            let corners = [(1, 1, 0, 2), (1, -1, 0, 3), (-1, 1, 1, 2), (-1, -1, 1, 3)];
            let diagonal = corners
                .iter()
                .filter(|&&(_, _, a, b)| level(a) && level(b))
                .filter_map(|&(dx, dz, _, _)| self.ender_neighbour(c, dx, dz));
            let next: Vec<[i32; 3]> = open.iter().copied().flatten().chain(diagonal).collect();
            for n in next {
                if came.contains_key(&n) {
                    continue;
                }
                came.insert(n, c);
                queue.push_back(n);
            }
        }
        // Back from the best node to the first step.
        let mut at = best.1;
        while let Some(&prev) = came.get(&at) {
            if prev == start {
                return (at != start).then_some(at);
            }
            at = prev;
        }
        None
    }

    /// Where the chase walks this tick (issue 18): the middle of the next cell on its path to
    /// the player, found again when the player's cell changes, when the mob reaches that cell, or
    /// every `PATH_EVERY` ticks (`MeleeAttackGoal` asks for a new path every 4 to 11 ticks).
    /// `None` at the end of the path: it stands.
    fn ender_waypoint(&self, e: &mut Ender, pos: [f64; 3]) -> Option<[f64; 2]> {
        let player = [
            self.pos[0].floor() as i32,
            (self.pos[1] + 1e-7).floor() as i32,
            self.pos[2].floor() as i32,
        ];
        let reached = |n: [i32; 3]| {
            (f64::from(n[0]) + 0.5 - pos[0]).hypot(f64::from(n[2]) + 0.5 - pos[2]) < 0.1
        };
        let stale = e.path_for != Some(player)
            || self.ticks >= e.path_at + PATH_EVERY
            || e.path_next.is_some_and(reached);
        if stale {
            e.path_next = self.ender_path(pos, player);
            e.path_for = Some(player);
            e.path_at = self.ticks;
        }
        let n = e.path_next?;
        Some([f64::from(n[0]) + 0.5, f64::from(n[2]) + 0.5])
    }

    /// The endermen's tick.
    pub(crate) fn tick_endermen(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let ids: Vec<i32> = self.survival.life.mobs.keys().copied().collect();
        for id in ids {
            let Some(m) = self.survival.life.mobs.get(&id).copied() else {
                continue;
            };
            let Brain::Enderman(mut e) = m.brain else {
                continue;
            };
            if m.dead_at.is_some() {
                continue;
            }
            // `checkDespawn` runs before the ticking check: a natural spawn past 128 is gone.
            if e.natural && dist_sq(m.pos, self.pos) > SPAWN_MAX_SQ {
                self.survival.life.mobs.remove(&id);
                out.push(wire(State::Play, &S2c::RemoveEntities { ids: vec![id] })?);
                continue;
            }
            if !self.ticking(m.pos) || self.health <= 0.0 {
                continue;
            }
            e.ticked += 1;
            let mut pos = m.pos;
            let mut moved = false;
            let mut meta = Vec::new();
            let d2 = dist_sq(self.pos, pos);
            let stared = self.stares_at(pos);
            // The target selector, whole on even ticks of its age.
            if (self.ticks - e.born).is_multiple_of(2) {
                if !e.angry {
                    match e.pending {
                        None if stared && d2 <= FOLLOW_RANGE * FOLLOW_RANGE => {
                            // `start`, then this tick's `tick`.
                            e.pending = Some(AGGRO_GOAL_TICKS - 1);
                            e.teleport_time = 0;
                            if !e.stared_at {
                                e.stared_at = true;
                                meta.push((META_STARED_AT, MetaValue::Bool(true)));
                            }
                        }
                        None => {}
                        Some(_) if !stared => e.pending = None,
                        Some(n) if n <= 1 => {
                            e.pending = None;
                            e.angry = true;
                            meta.push((META_CREEPY, MetaValue::Bool(true)));
                        }
                        Some(n) => e.pending = Some(n - 1),
                    }
                } else if stared {
                    if d2 < NEAR_SQ
                        && let Some(w) = self.teleport_random(id, pos)?
                    {
                        out.push(w);
                        moved = true;
                    }
                    e.teleport_time = 0;
                } else if d2 > FAR_SQ {
                    let t = e.teleport_time;
                    e.teleport_time += 1;
                    if t >= TELEPORT_GOAL_TICKS
                        && let Some(w) = self.teleport_toward(id, pos)?
                    {
                        out.push(w);
                        moved = true;
                        e.teleport_time = 0;
                    }
                }
            }
            if moved {
                pos = self.survival.life.mobs.get(&id).map_or(pos, |mm| mm.pos);
                e.teleports += 1;
                e.stroll = None;
                e.path_next = None;
                e.path_for = None;
                e.vy = 0.0;
            }
            // Falling.
            if !self.floored(pos) {
                e.vy = (e.vy - physics::GRAVITY) * physics::VERTICAL_DRAG;
                let mut y = pos[1] + e.vy;
                let (x, z) = (pos[0].floor() as i32, pos[2].floor() as i32);
                let mut c = pos[1].floor() as i32;
                while f64::from(c) > y {
                    if self.solid(x, c - 1, z).is_some() {
                        y = f64::from(c);
                        e.vy = 0.0;
                        break;
                    }
                    c -= 1;
                }
                if self.state_at(x, y.floor() as i32, z).is_none() && y < pos[1] {
                    // Out of the world.
                    self.survival.life.mobs.remove(&id);
                    out.push(wire(State::Play, &S2c::RemoveEntities { ids: vec![id] })?);
                    continue;
                }
                pos[1] = y;
                moved = true;
            } else {
                e.vy = 0.0;
                // The goals that move it.
                let frozen = e.angry && stared && d2 <= FAR_SQ;
                let goal = if frozen {
                    None
                } else if e.angry {
                    e.stroll = None;
                    self.ender_waypoint(&mut e, pos).map(|g| (g, CHASE_SPEED))
                } else {
                    if e.stroll.is_none()
                        && (self.ticks - e.born).is_multiple_of(2)
                        && self.survival.next_double() < 1.0 / STROLL_EVERY
                    {
                        let x = pos[0] + (self.survival.next_double() * 2.0 - 1.0) * STROLL_RANGE;
                        let z = pos[2] + (self.survival.next_double() * 2.0 - 1.0) * STROLL_RANGE;
                        let land = (-STROLL_DY..=STROLL_DY).any(|dy| {
                            let y = pos[1].floor() + f64::from(dy);
                            self.ender_fits([x, y, z]) && self.floored([x, y, z])
                        });
                        if land {
                            e.stroll = Some([x, z]);
                        }
                    }
                    e.stroll.map(|g| (g, STROLL_SPEED))
                };
                if let Some((g, speed)) = goal {
                    let reach = e.angry && self.in_attack_reach(pos);
                    if !reach {
                        match self.ender_step(pos, g, speed) {
                            Some(p) => {
                                pos = p;
                                moved = true;
                            }
                            None => e.stroll = None,
                        }
                    }
                }
            }
            // `MeleeAttackGoal.canPerformAttack`: in the attack box, and seen (`Sensing.hasLineOfSight`,
            // eye to eye: issue 18's burrow walls stop a blow through them). The sampled `sight`, not
            // an exact walk, on purpose: on the real server the bot in M under the roof was hit in 3 of
            // 5 runs (e08-near) though the line from O's middle clips the roof; the samples let a blow
            // through there too, so the model is never kinder than the game at M (an exact walk was).
            let seen = || {
                self.sight(
                    [pos[0], pos[1] + EYE, pos[2]],
                    [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]],
                )
            };
            if e.angry && self.in_attack_reach(pos) && self.ticks >= m.next_hit && seen() {
                if let Some(mm) = self.survival.life.mobs.get_mut(&id) {
                    mm.next_hit = self.ticks + ATTACK_EVERY;
                }
                let (_, hit) = self.ender_blow(id, pos)?;
                out.extend(hit);
            }
            if let Some(mm) = self.survival.life.mobs.get_mut(&id) {
                mm.pos = pos;
                mm.brain = Brain::Enderman(e);
            }
            if moved {
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
            if !meta.is_empty() {
                out.push(wire(
                    State::Play,
                    &S2c::SetEntityData { id, entries: meta },
                )?);
            }
        }
        Ok(out)
    }

    /// `isWithinMeleeAttackRange`: its box grown 0.83 sideways meets the player's box.
    fn in_attack_reach(&self, at: [f64; 3]) -> bool {
        let reach = HALF_WIDTH + ATTACK_REACH + physics::HALF_WIDTH;
        (at[0] - self.pos[0]).abs() <= reach
            && (at[2] - self.pos[2]).abs() <= reach
            && self.pos[1] < at[1] + HEIGHT
            && at[1] < self.pos[1] + physics::HEIGHT
    }

    /// The player's hit on an enderman: it takes the player as its target
    /// (`HurtByTargetGoal`), and a hit by a living entity never teleports it.
    pub(crate) fn ender_hurt_by_player(&mut self, id: i32) -> Result<Vec<Wire>, EncodeError> {
        let Some(m) = self.survival.life.mobs.get_mut(&id) else {
            return Ok(Vec::new());
        };
        let Brain::Enderman(e) = &mut m.brain else {
            return Ok(Vec::new());
        };
        if e.angry {
            return Ok(Vec::new());
        }
        e.angry = true;
        e.pending = None;
        Ok(vec![wire(
            State::Play,
            &S2c::SetEntityData {
                id,
                entries: vec![(META_CREEPY, MetaValue::Bool(true))],
            },
        )?])
    }

    /// The endermen's natural spawning in warped forests (`NaturalSpawner`).
    pub(crate) fn tick_ender_spawns(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        if !self.survival.life.natural_spawns || self.config.peaceful || self.health <= 0.0 {
            return Ok(out);
        }
        let chunks: Vec<(i32, i32)> = self
            .world
            .columns
            .keys()
            .copied()
            .filter(|&(cx, cz)| {
                let mid = [f64::from(cx * 16 + 8), self.pos[1], f64::from(cz * 16 + 8)];
                self.ticking(mid)
                    && (mid[0] - self.pos[0]).powi(2) + (mid[2] - self.pos[2]).powi(2)
                        < SPAWN_MAX_SQ
            })
            .collect();
        for (cx, cz) in chunks {
            let monsters = self
                .survival
                .life
                .mobs
                .values()
                .filter(|m| m.dead_at.is_none())
                .count();
            if monsters >= MONSTER_CAP {
                break;
            }
            // `getRandomPosWithin`: x and z in the chunk, y up to its top block + 1.
            let x0 = cx * 16 + (self.survival.next_double() * 16.0).floor() as i32;
            let z0 = cz * 16 + (self.survival.next_double() * 16.0).floor() as i32;
            let Some(col) = self.world.columns.get(&(cx, cz)) else {
                continue;
            };
            let (min_y, max_y) = (col.column.min_y, col.column.max_y());
            let top = (min_y..max_y)
                .rev()
                .find(|&y| {
                    self.state_at(x0, y, z0)
                        .and_then(data::block_of_state)
                        .is_some_and(|k| k != BlockKind::Air)
                })
                .unwrap_or(min_y);
            let span = f64::from(top + 1 - min_y + 1);
            let y0 = min_y + (self.survival.next_double() * span).floor() as i32;
            if self.solid(x0, y0, z0) == Some(1.0) {
                continue;
            }
            // `spawnCategoryForPosition`: three packs; a pack's type is drawn at its first try that
            // is the right distance from the player, from the biome there, and its group size
            // then becomes the pack's tries; the whole call stops at the cluster size.
            let mut total = 0;
            'packs: for _ in 0..PACKS {
                let (mut x, mut z) = (x0, z0);
                let mut tries = (self.survival.next_double() * PACK_TRIES).ceil() as u32;
                let mut chosen: Option<EntityKind> = None;
                let mut q = 0;
                while q < tries {
                    q += 1;
                    let mut step = || {
                        ((self.survival.next_double() * PACK_STEP).floor()
                            - (self.survival.next_double() * PACK_STEP).floor())
                            as i32
                    };
                    x += step();
                    z += step();
                    let at = [f64::from(x) + 0.5, f64::from(y0), f64::from(z) + 0.5];
                    let d2 = dist_sq(at, self.pos);
                    if d2 <= SPAWN_MIN_SQ || d2 > SPAWN_MAX_SQ {
                        continue;
                    }
                    let biome = self.world.biome_at(x, y0, z);
                    let kind = match chosen {
                        Some(k) => k,
                        None => {
                            let Some((k, group)) = self.draw_monster(biome) else {
                                break;
                            };
                            chosen = Some(k);
                            tries = group;
                            k
                        }
                    };
                    // The model spawns endermen only: a pack of another kind is left out.
                    if kind != EntityKind::Enderman {
                        continue 'packs;
                    }
                    if !monsters_of(biome).iter().any(|m| m.0 == kind) {
                        continue;
                    }
                    if self.solid(x, y0 - 1, z).is_none() || !self.ender_fits(at) {
                        continue;
                    }
                    if let Some((charge, budget)) = spawn_cost(biome, kind) {
                        let potential: f64 = self
                            .survival
                            .life
                            .mobs
                            .values()
                            .filter(|m| m.dead_at.is_none() && m.kind == EntityKind::Enderman)
                            .map(|m| {
                                let d = dist_sq(m.pos, at).sqrt();
                                if d < 1e-9 {
                                    f64::INFINITY
                                } else {
                                    charge * charge / d
                                }
                            })
                            .sum();
                        if potential > budget {
                            continue;
                        }
                    }
                    let id = self.summon_enderman(at);
                    if let Some(Mob {
                        brain: Brain::Enderman(e),
                        ..
                    }) = self.survival.life.mobs.get_mut(&id)
                    {
                        e.natural = true;
                    }
                    self.survival.life.ender_spawns += 1;
                    let mut uuid = [0u8; 16];
                    uuid[12..].copy_from_slice(&id.to_be_bytes());
                    out.push(wire(
                        State::Play,
                        &S2c::AddEntity {
                            id,
                            uuid,
                            kind: EntityKind::Enderman.id(),
                            x: at[0],
                            y: at[1],
                            z: at[2],
                        },
                    )?);
                    total += 1;
                    if total >= CLUSTER {
                        break 'packs;
                    }
                }
            }
        }
        Ok(out)
    }

    /// `getRandomSpawnMobAt`: a monster drawn by weight from the biome's list, and its group
    /// size (`minCount` plus `nextInt(maxCount - minCount + 1)`).
    fn draw_monster(&mut self, biome: Option<&str>) -> Option<(EntityKind, u32)> {
        let list = monsters_of(biome);
        let total: u32 = list.iter().map(|m| m.1).sum();
        if total == 0 {
            return None;
        }
        let mut pick = (self.survival.next_double() * f64::from(total)).floor() as u32;
        let &(kind, _, min, max) = list.iter().find(|m| {
            if pick < m.1 {
                true
            } else {
                pick -= m.1;
                false
            }
        })?;
        let group = min + (self.survival.next_double() * f64::from(max - min + 1)).floor() as u32;
        Some((kind, group))
    }

    /// An enderman's blow (7, `mob_attack`), through a raised shield's rule.
    fn ender_blow(&mut self, id: i32, at: [f64; 3]) -> Result<(bool, Vec<Wire>), EncodeError> {
        // The damage event names the enderman as cause and direct (`mob_attack` from a mob), as
        // the real server sends it.
        self.survival.life.blow_cause = Some(id);
        let blow = self.blow_from(id, ATTACK, at, 0);
        self.survival.life.blow_cause = None;
        let (landed, out) = blow?;
        if landed {
            *self
                .survival
                .life
                .hurt_by_mob
                .entry("enderman".to_owned())
                .or_insert(0) += 1;
        }
        Ok((landed, out))
    }
}
