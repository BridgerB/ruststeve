//! Bot — connects, logs in, runs a 50 ms physics tick (movement sim + position
//! packets), tracks world/inventory/entities, and exposes high-level actions
//! (block queries, controls, look, dig, pathfinding `goto`). Faithful-in-spirit
//! port of typecraft's event-driven `bot`, adapted to a single-task Rust model:
//! every action drives [`Bot::drive_tick`], which races packet reads against the
//! 50 ms physics deadline, so keep-alive + physics keep running while waiting.

mod conversions;
mod crafting;
mod inventory;

pub use conversions::*;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::block::{state_id_to_block, BlockInfo};
use crate::chunk::{ChunkColumn, ChunkColumnOptions, GLOBAL_BITS_PER_BIOME, GLOBAL_BITS_PER_BLOCK};
use crate::entity::Entity;
use crate::item::{from_notch, Item};
use crate::path::{AStar, Goal, GoalNear, GoalNearXZ, Move, Movements, MovementsConfig, PathResult, PathStatus};
use crate::physics::{
    apply_player_state, create_player_state, PhysicsEngine, PhysicsWorld, PlayerControls,
    WorldPhysics,
};
use crate::protocol::{Client, ClientOptions, PValue};
use crate::registry::Registry;
use crate::vec3::{vec3, Vec3};
use crate::window::Window;
use crate::world::{raycast, World, PLAYER_EYE_HEIGHT};

const TICK: Duration = Duration::from_millis(50);

/// A block face for digging/placing.
#[derive(Debug, Clone, Copy)]
pub enum Face {
    Bottom = 0,
    Top = 1,
    North = 2,
    South = 3,
    West = 4,
    East = 5,
}

#[derive(Debug, Clone, Default)]
pub struct GameInfo {
    pub game_mode: String,
    pub dimension: String,
    pub difficulty: String,
    pub hardcore: bool,
    pub max_players: i32,
    pub min_y: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Default)]
pub struct TimeInfo {
    pub age: i64,
    pub time_of_day: i64,
    pub day: i64,
    pub is_day: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ControlState {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sprint: bool,
    pub sneak: bool,
}

/// Result of following one computed path segment.
enum FollowOutcome {
    /// Reached the end of the path.
    Reached,
    /// Stuck or blocked — the caller should recompute a path.
    NeedRepath,
    /// Took damage while walking (lava/fire the local world didn't show) — abort the whole goto
    /// so the caller's survival logic (retreat + heal) runs instead of the follower re-pathing
    /// straight back into it.
    Hurt,
    Disconnected,
}

/// Outcome of one [`Bot::drive_tick`].
#[derive(Debug, Clone)]
pub enum DriveStep {
    /// The connection closed.
    Disconnected,
    /// A 50 ms physics tick ran.
    Tick,
    /// A packet was handled internally with no surfaced event.
    Handled,
    /// A high-level event occurred.
    Event(BotEvent),
}

/// High-level events surfaced from [`Bot::next_event`].
#[derive(Debug, Clone)]
pub enum BotEvent {
    /// A 50 ms physics tick elapsed.
    Tick,
    Login,
    Spawn,
    Death,
    Health,
    /// Server-forced teleport.
    ForcedMove,
    ChunkLoad(i32, i32),
    BlockUpdate(i32, i32, i32),
    EntitySpawn(i32),
    Chat(String),
    Inventory,
    Kicked(String),
    Packet(String),
}

/// Process-wide mirror of `Bot::deaths` (one bot per process), readable while a task holds
/// `&mut Bot`. The gym's death-loop guard watches it during a running step.
pub static DEATH_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub struct Bot<'a> {
    pub client: Client,
    pub registry: &'a Registry,
    /// The bot's own entity (position/velocity/yaw/pitch/on_ground/effects).
    pub entity: Entity,
    pub game: GameInfo,
    pub health: f64,
    pub food: f64,
    pub food_saturation: f64,
    pub spawn_point: Vec3,
    pub time: TimeInfo,
    pub world: World<'a>,
    pub entities: HashMap<i32, Entity>,
    pub inventory: Window,
    /// Currently-open container window (chest/furnace/crafting table), if any.
    pub current_window: Option<Window>,
    pub held_slot: i32,
    pub control_state: ControlState,
    pub physics_enabled: bool,
    /// Movement rules for pathfinding. Defaults to `max_drop_down = 1` so the
    /// bot never takes a one-way drop it can't climb back up (surface-safe);
    /// raise it for tasks that deliberately descend (e.g. mining).
    pub movement: MovementsConfig,

    next_action_id: i32,
    /// Bumped on every inventory/window slot update from the server.
    inv_revision: u32,
    physics: Option<PhysicsEngine>,
    should_physics: bool,
    last_tick: Instant,
    /// Throttle for the optional top-down world snapshot (RUST_VIEW dashboard feed).
    view_last: Instant,
    /// Shared snapshot fed to the live 3D-viewer SSE server (Some only under RUST_VIEW).
    pub viewer: Option<crate::viewer::ViewerHandle>,
    /// When the server last teleported/corrected our position. Digs wait for this
    /// to be stale (position agreed) before mining, else they're out-of-reach.
    last_teleport: Instant,
    last_sent: Option<(Vec3, f64, f64)>,
    sequence: i32,
    spawned: bool,
    alive: bool,
    brand: String,
    /// BREATH WATCHDOG (cycle 2). Survival reflexes only ran BETWEEN steps, so a bot whose head
    /// went under mid-step (pathing to wood, digging into an aquifer) drowned inside the step —
    /// 8 race drownings to 1 lava death in cycle 1. The tick driver now holds `jump` (the physics
    /// gives +0.04/tick lift in water) once the head has been under for HOLD_JUMP_AFTER, unless a
    /// step declares water work (`allow_underwater`); still under after ALARM_AFTER → `breath_alarm`
    /// makes movement primitives return so the main loop's survival escape (cap-dig) runs.
    pub allow_underwater: bool,
    pub breath_alarm: bool,
    /// Explosions the server reported (`explode` packets) since connect: the only ground truth on the
    /// client that a used bed went off, as opposed to being destroyed by a dragon part.
    pub explosions: u32,
    /// Pre-emptions since the last drain (the main loop writes them to race.db).
    pub breath_preempts: u32,
    underwater_since: Option<Instant>,
    /// True while the watchdog (not a task) is the one holding `jump` — released on surfacing.
    watchdog_jump: bool,
    heartbeat_last: Instant,
    /// Deaths since connect, counted at the health-packet edge (see the update_health handler).
    pub deaths: u32,
    wet_since: Option<Instant>,
    /// Set by the water escape (leave_water) so its digs run while `breath_alarm` blocks tasks'.
    pub escaping: bool,
    /// Set by the escape only while a route leg dives (jump released on purpose).
    pub escape_diving: bool,
    /// LAVA SIGHTINGS (cycle 6, Part 3): every lava SOURCE the bot has seen with an air face, with the
    /// time (ms since epoch) it was first seen. Filled from the loaded chunks every few seconds by the tick
    /// driver during every step, so lava passed at minute 40 is still known at the portal step. Persists
    /// across deaths; cleared on a dimension change (a new world). Site selection reads only this.
    pub lava_seen: HashMap<(i32, i32, i32), u64>,
    lava_scan_last: Instant,
}

fn block_pos(x: i32, y: i32, z: i32) -> PValue {
    PValue::compound(vec![
        ("x", PValue::num(x as f64)),
        ("y", PValue::num(y as f64)),
        ("z", PValue::num(z as f64)),
    ])
}

/// Last SDK entry point the bot went through — printed with LOOP STALL (stall hunt, 05:45:
/// four stalls of 14.05/14.05/18.11/18.11 s after descent steps, none in A* or physics_tick).
pub static STALL_SITE: std::sync::Mutex<&'static str> = std::sync::Mutex::new("-");
fn site(s: &'static str) {
    *STALL_SITE.lock().unwrap() = s;
}

impl<'a> Bot<'a> {
    /// Connect, log in, and advance to PLAY.
    pub async fn connect(options: ClientOptions, registry: &'a Registry) -> std::io::Result<Bot<'a>> {
        let mut client = Client::connect(&options.host, options.port, &options.username).await?;
        client.login(&options).await?;
        let mut entity = Entity::new(0);
        entity.health = 20.0;
        // Pickaxe-required blocks the bot can't break by hand. The pathfinder
        // must NOT route through these (it would tunnel "through" stone the bot
        // can never mine, and the follower would wedge digging it forever). The
        // mining task clears this once a pickaxe is in hand.
        let mut cant_break: std::collections::HashSet<i32> = std::collections::HashSet::new();
        for (name, def) in &registry.blocks_by_name {
            if name.contains("stone") || name.contains("ore") || name.contains("deepslate")
                || name.contains("obsidian") || name.contains("basalt") || name.contains("blackstone")
                || name.contains("granite") || name.contains("diorite") || name.contains("andesite")
                || name.contains("tuff") || name.contains("calcite") || name.contains("terracotta")
                || name.contains("brick") || name.contains("ancient_debris")
            {
                cant_break.insert(def.id);
            }
        }
        // Permanent no-dig set: survives every `blocks_cant_break.clear()`. Obsidian is the bot's
        // own portal frame (an iron pick CAN break it in 250s and the pathfinder's flat dig cost
        // would happily route through it); bedrock is unbreakable.
        let never_break: std::collections::HashSet<i32> = registry
            .blocks_by_name
            .iter()
            .filter(|(name, _)| name.contains("obsidian") || name.as_str() == "bedrock")
            .map(|(_, def)| def.id)
            .collect();
        Ok(Bot {
            inventory: crate::window::create_window_from_type(registry, 0, -1, Some("minecraft:inventory"), "Inventory", None)
                .unwrap_or_else(|| Window::new(0, "minecraft:inventory", "", 46, 9, 44, 0, true)),
            current_window: None,
            next_action_id: 0,
            inv_revision: 0,
            world: World::new(registry),
            registry,
            entity,
            game: GameInfo {
                game_mode: "survival".into(),
                dimension: "overworld".into(),
                difficulty: "normal".into(),
                min_y: -64,
                height: 384,
                ..Default::default()
            },
            health: 20.0,
            food: 20.0,
            food_saturation: 5.0,
            spawn_point: Vec3::new(0.0, 0.0, 0.0),
            time: TimeInfo::default(),
            entities: HashMap::new(),
            held_slot: 0,
            control_state: ControlState::default(),
            physics_enabled: true,
            movement: MovementsConfig { liquid_cost: 100.0, max_drop_down: 1, blocks_cant_break: cant_break, blocks_never_break: never_break, ..MovementsConfig::default() }, // low drop + don't path through unbreakable stone
            physics: None,
            should_physics: false,
            last_tick: Instant::now(),
            allow_underwater: false,
            breath_alarm: false,
            explosions: 0,
            breath_preempts: 0,
            underwater_since: None,
            watchdog_jump: false,
            heartbeat_last: Instant::now() - std::time::Duration::from_secs(60),
            deaths: 0,
            wet_since: None,
            escaping: false,
            escape_diving: false,
            lava_seen: HashMap::new(),
            lava_scan_last: Instant::now(),
            view_last: Instant::now(),
            viewer: {
                if std::env::var("RUST_VIEW").is_ok() {
                    let handle: crate::viewer::ViewerHandle =
                        std::sync::Arc::new(std::sync::Mutex::new(crate::viewer::ViewerShared {
                            version: registry.version.minecraft_version.clone(),
                            min_y: -64,
                            height: 384,
                            time: 1000,
                            ..Default::default()
                        }));
                    let port = crate::viewer::port_for(&options.username);
                    let assets = std::env::var("RUST_VIEW_ASSETS")
                        .unwrap_or_else(|_| "viewer/static/assets.json".into());
                    crate::viewer::spawn_server(port, handle.clone(), assets);
                    Some(handle)
                } else {
                    None
                }
            },
            last_teleport: Instant::now(),
            last_sent: None,
            sequence: 0,
            spawned: false,
            alive: true,
            brand: "vanilla".into(),
            client,
        })
    }

    pub fn username(&self) -> &str {
        &self.client.username
    }

    /// Held item (main hand).
    pub fn held_item(&self) -> Option<&Item> {
        self.inventory.slots.get(36 + self.held_slot as usize).and_then(|s| s.as_ref())
    }

    // ── Core drive loop ──

    /// Advance one step: handle a packet if one arrives before the 50 ms physics
    /// deadline, otherwise run a physics tick.
    pub async fn drive_tick(&mut self) -> std::io::Result<DriveStep> {
        // Live-3D-viewer feed for the dashboard. POSE is written EVERY tick (cheap — 5 floats
        // under a lock) so the streamed camera is smooth; the EXPENSIVE chunk-dump stays throttled
        // and skipped while a container window is open (a heavy per-tick scan regressed craft
        // timing before). Updating pose only every few seconds made the feed a slideshow.
        if self.viewer.is_some() {
            self.update_viewer_pose();
            if self.current_window.is_none()
                && self.view_last.elapsed() >= std::time::Duration::from_secs(1)
            {
                self.view_last = Instant::now();
                self.update_viewer();
            }
        }
        let elapsed = self.last_tick.elapsed();
        // STALL detector: the loop is supposed to come back here every ≤50 ms. rust-gym-001 was
        // kicked with `disconnect.timeout` (no keep-alive answer for ~30 s) twice tonight with
        // nothing in the log — some synchronous section blocked the loop. Print any gap > 3 s so
        // the lines around it show which call it was.
        if elapsed >= std::time::Duration::from_secs(3) {
            eprintln!("[bot] LOOP STALL {} ms without driving the connection (last site: {})", elapsed.as_millis(), *STALL_SITE.lock().unwrap());
        }
        // HEARTBEAT for the race watchdog: race.db ticks are written once per STEP (minutes apart in
        // a long walk), so a stale tick is not a hang — cycle-2's first smoke race false-killed a
        // healthy bot 97 s into gather_wood. This file is touched every 10 s while the tick loop
        // runs; race-b.sh kills only when it is older than STALE_SECS.
        if self.heartbeat_last.elapsed() >= std::time::Duration::from_secs(10) {
            self.heartbeat_last = Instant::now();
            let _ = std::fs::write(format!(".heartbeat-{}", self.username()), b"");
        }
        if self.lava_scan_last.elapsed() >= std::time::Duration::from_secs(5) {
            self.lava_scan_last = Instant::now();
            self.record_lava_sightings();
        }
        if elapsed >= TICK {
            self.breath_watchdog();
            let t_phys = Instant::now();
            self.physics_tick().await?;
            if t_phys.elapsed() > std::time::Duration::from_millis(500) {
                eprintln!("[bot] SLOW physics_tick {} ms at ({:.1},{:.1},{:.1})", t_phys.elapsed().as_millis(), self.entity.position.x, self.entity.position.y, self.entity.position.z);
            }
            self.last_tick = Instant::now();
            return Ok(DriveStep::Tick);
        }
        match tokio::time::timeout(TICK - elapsed, self.client.next_packet()).await {
            Ok(Ok(Some((name, params)))) => match self.handle_packet(&name, &params).await? {
                Some(ev) => Ok(DriveStep::Event(ev)),
                None => Ok(DriveStep::Handled),
            },
            Ok(Ok(None)) => {
                // EXIT on a lost connection. Every wait/dig/goto helper treats Disconnected as
                // "stop early, Ok(())", and the task code discards those results — so a dropped
                // bot kept looping on its frozen local world at ~50 iterations/s: rust-gym-001 ran
                // ~70 min as a ghost ("spreadplayers: No entity was found"), 116k `desc STUCK`
                // lines and two fake 2400 s timeouts. The drivers (race-b.sh, the gym scripts)
                // relaunch or record a dead process; a ghost looks alive to all of them.
                eprintln!("[bot] CONNECTION LOST (server closed the stream) — exiting (code 3)");
                std::process::exit(3);
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                self.breath_watchdog();
                self.physics_tick().await?;
                self.last_tick = Instant::now();
                Ok(DriveStep::Tick)
            }
        }
    }

    /// Is the bot's head (eye block) in water right now?
    pub fn head_submerged(&self) -> bool {
        let p = self.entity.position;
        let (x, hy, z) = (p.x.floor() as i32, (p.y + 1.62).floor() as i32, p.z.floor() as i32);
        self.block_at(x, hy, z).map(|b| b.name.contains("water")).unwrap_or(false)
    }

    /// Per-tick breath guard, above every step (see the field docs). Holding jump is the only
    /// action it takes itself; the escalation (`breath_alarm`) is acted on by the main loop.
    fn breath_watchdog(&mut self) {
        const HOLD_JUMP_AFTER: std::time::Duration = std::time::Duration::from_millis(1000);
        const ALARM_AFTER: std::time::Duration = std::time::Duration::from_millis(6000);
        // FEET in water too long (floating at the surface of a deep lake): the head is out, so
        // the breath clock never runs — but the pathfinder now refuses deep water, so the step
        // can't walk out either (cycle-2 water_lake trial 2: floated 120 s). Alarm after 8 s so
        // the main loop's leave_water swims to the bank. 30 s, not 8: a normal swim across a lake takes
        // well under that, and the pathfinder swims out better than leave_water when it can.
        let p = self.entity.position;
        // Wading (on the ground in a shallow layer) is not floating: it never drowns and the
        // pathfinder walks out of it, so it must not start the 30 s clock or hold the alarm on.
        let feet_wet = !self.entity.on_ground
            && self
                .block_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
                .map(|b| b.name.contains("water"))
                .unwrap_or(false);
        if feet_wet && !self.allow_underwater {
            let since = *self.wet_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= std::time::Duration::from_secs(30) && !self.breath_alarm {
                self.breath_alarm = true;
                eprintln!("[bot] WATER ALARM — feet in water {:.0}s at ({:.1},{:.1},{:.1})", since.elapsed().as_secs_f64(), p.x, p.y, p.z);
            }
        } else if !feet_wet {
            self.wet_since = None;
        }
        if !self.head_submerged() {
            if !feet_wet && self.wet_since.is_none() {
                self.breath_alarm = false;
            }
            if self.underwater_since.take().is_some() {
                // Head out: release the watchdog's jump. The alarm clears only once the feet are
                // dry too (above) — a bot bobbing at a deep-lake surface still has to get out.
                if self.watchdog_jump {
                    self.control_state.jump = false;
                    self.watchdog_jump = false;
                }
            }
            return;
        }
        let since = *self.underwater_since.get_or_insert_with(Instant::now);
        if self.allow_underwater {
            return;
        }
        let under = since.elapsed();
        // Yield only while an escape route leg DIVES (out of a roofed aquifer, race i4 06:09): a
        // forced jump pins the bot against the roof. Not for the whole escape — its bank swim
        // relies on this held jump to stay afloat (water_lake drowned when it yielded throughout).
        if under >= HOLD_JUMP_AFTER && !self.escape_diving {
            if !self.control_state.jump {
                self.breath_preempts += 1;
                self.watchdog_jump = true;
            }
            self.control_state.jump = true;
        }
        if under >= ALARM_AFTER && !self.breath_alarm {
            self.breath_alarm = true;
            eprintln!("[bot] BREATH ALARM — head under water {:.1}s at ({:.1},{:.1},{:.1})", under.as_secs_f64(), self.entity.position.x, self.entity.position.y, self.entity.position.z);
        }
    }

    /// Drive until a high-level event occurs. Returns `None` on disconnect.
    pub async fn next_event(&mut self) -> std::io::Result<Option<BotEvent>> {
        loop {
            match self.drive_tick().await? {
                DriveStep::Disconnected => return Ok(None),
                DriveStep::Event(ev) => return Ok(Some(ev)),
                DriveStep::Tick | DriveStep::Handled => continue,
            }
        }
    }

    /// Drive the loop for `n` physics ticks (stops early on disconnect).
    pub async fn wait_ticks(&mut self, n: u32) -> std::io::Result<()> {
        site("wait_ticks");
        let mut ticks = 0;
        while ticks < n {
            match self.drive_tick().await? {
                DriveStep::Disconnected => return Ok(()),
                DriveStep::Tick => ticks += 1,
                _ => {}
            }
        }
        Ok(())
    }

    /// Drive the loop for AT LEAST `ms` of real wall-clock time, pumping packets so
    /// world/entity state stays fresh. Unlike [`wait_ticks`], an incoming-packet flood
    /// can't collapse the delay: `wait_ticks(14)` returned in <2ms during combat (the
    /// packet backlog satisfied 14 "ticks" instantly), so the bot dumped 40 swings in
    /// 66ms — all but the first landed inside a mob's 10-tick (0.5s) hurt-invulnerability
    /// and dealt zero damage. Combat pacing needs a guaranteed real interval, so this
    /// floors on the clock: it keeps driving (draining packets, ignoring transient drive
    /// errors) until `ms` has actually elapsed, then returns.
    pub async fn wait_real_ms(&mut self, ms: u64) -> std::io::Result<()> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, self.drive_tick()).await {
                Err(_) => break,                                  // real deadline reached — the floor
                Ok(Ok(DriveStep::Disconnected)) => return Ok(()), // connection gone: stop early
                Ok(Ok(_)) => {}                                   // tick/packet handled: keep going
                Ok(Err(_)) => tokio::time::sleep(Duration::from_millis(5)).await, // transient: don't spin
            }
        }
        Ok(())
    }

    // ── Physics tick ──

    async fn physics_tick(&mut self) -> std::io::Result<()> {
        if !self.physics_enabled || !self.should_physics {
            return Ok(());
        }
        if self.physics.is_none() {
            self.physics = Some(PhysicsEngine::new(self.registry));
        }
        // No physics in an UNLOADED chunk (vanilla's client doesn't move the player there). With no
        // blocks to collide with, gravity free-fell the bot through the floor into the void after a
        // respawn teleport, and the server killed it below the world every ~7 s (batch 3 rust-gym-002:
        // "fell out of the world", 234 deaths in one trial). Hold position until the column arrives.
        {
            let p = self.entity.position;
            let (cx, cz) = ((p.x.floor() as i32).div_euclid(16), (p.z.floor() as i32).div_euclid(16));
            if !self.world.columns.contains_key(&(cx, cz)) {
                self.entity.velocity = crate::vec3::vec3(0.0, 0.0, 0.0);
                return self.send_position().await;
            }
        }

        let controls = PlayerControls {
            forward: self.control_state.forward,
            back: self.control_state.back,
            left: self.control_state.left,
            right: self.control_state.right,
            jump: self.control_state.jump,
            sprint: self.control_state.sprint,
            sneak: self.control_state.sneak,
        };
        let mut state = create_player_state(self.registry, &self.entity, controls);
        {
            let pw = WorldPhysics::new(&self.world);
            let engine = self.physics.as_ref().unwrap();
            engine.simulate_player(&mut state, &pw as &dyn PhysicsWorld);
        }
        apply_player_state(&state, &mut self.entity);

        self.send_position().await
    }

    async fn send_position(&mut self) -> std::io::Result<()> {
        let pos = self.entity.position;
        let yaw = self.entity.yaw;
        let pitch = self.entity.pitch;
        let on_ground = self.entity.on_ground;

        let (pos_changed, look_changed) = match self.last_sent {
            Some((p, y, pi)) => (p != pos, y != yaw || pi != pitch),
            None => (true, true),
        };
        if !pos_changed && !look_changed {
            return Ok(());
        }
        let flags = PValue::compound(vec![("onGround", PValue::Bool(on_ground))]);

        if pos_changed && look_changed {
            self.client
                .write(
                    "move_player_pos_rot",
                    PValue::compound(vec![
                        ("x", PValue::num(pos.x)),
                        ("y", PValue::num(pos.y)),
                        ("z", PValue::num(pos.z)),
                        ("yaw", PValue::num(to_notchian_yaw(yaw))),
                        ("pitch", PValue::num(to_notchian_pitch(pitch))),
                        ("flags", flags),
                    ]),
                )
                .await?;
        } else if pos_changed {
            self.client
                .write(
                    "move_player_pos",
                    PValue::compound(vec![
                        ("x", PValue::num(pos.x)),
                        ("y", PValue::num(pos.y)),
                        ("z", PValue::num(pos.z)),
                        ("flags", flags),
                    ]),
                )
                .await?;
        } else {
            self.client
                .write(
                    "move_player_rot",
                    PValue::compound(vec![
                        ("yaw", PValue::num(to_notchian_yaw(yaw))),
                        ("pitch", PValue::num(to_notchian_pitch(pitch))),
                        ("flags", flags),
                    ]),
                )
                .await?;
        }
        self.last_sent = Some((pos, yaw, pitch));
        Ok(())
    }

    // ── Packet handling ──

    async fn handle_packet(&mut self, name: &str, params: &PValue) -> std::io::Result<Option<BotEvent>> {
        if std::env::var("PKT_DEBUG").is_ok()
            && (name.contains("block") || name.contains("ack") || name.contains("position") || name.contains("disconnect") || name == "system_chat")
        {
            eprintln!("PKT {name} {:?}", params);
        }
        match name {
            "login" => {
                self.handle_login(params).await?;
                return Ok(Some(BotEvent::Login));
            }
            // Dimension change (e.g. stepping through a nether portal). The server
            // sends `respawn` with the new world's SpawnInfo; update our dimension so
            // callers can detect the change. Without this `game.dimension` would stay
            // "overworld" forever and entering the Nether would be invisible.
            "respawn" => {
                // The server closes any open container on respawn (death or dimension change); a window
                // still recorded here would take later inventory clicks (see Bot::craft).
                if self.current_window.is_some() && crate::bot::crafting::stale_window_fix() {
                    self.sync_window_to_inventory();
                }
                if let Some(dim) = params
                    .get("worldState")
                    .and_then(|w| w.get("name"))
                    .and_then(PValue::as_str)
                {
                    // A DIMENSION CHANGE invalidates every loaded column: chunk keys are just (cx,cz),
                    // so overworld columns (min_y -64) kept answering nether lookups until the server
                    // happened to resend that chunk. Harness 09:00: rust-nether-001 reported fortress
                    // brick at (4,-3,147) — RCON: nether_bricks at (4,61,147), exactly 64 higher.
                    if self.game.dimension != dim {
                        self.world.columns.clear();
                        self.entities.clear();
                        self.lava_seen.clear();
                    }
                    self.game.dimension = dim.to_string();
                    // Update world height for the NEW dimension so chunk parsing reads the right
                    // number of 16-block sections. The overworld is 384 tall (24 sections); the
                    // nether and end are 256 (16 sections). Without this, entering the nether kept
                    // the overworld's 24-section count and the parser ran off the end of the
                    // (shorter) nether chunk buffer → panic at chunk_section.rs (index == len).
                    let (min_y, height) = if dim.contains("nether") || dim.contains("the_end") {
                        (0, 256)
                    } else {
                        (-64, 384)
                    };
                    self.game.min_y = min_y;
                    self.game.height = height;
                }
                return Ok(Some(BotEvent::Login));
            }
            "keep_alive" => {
                let id = params.get("keepAliveId").cloned().unwrap_or(PValue::Long(0));
                self.client.write("keep_alive", PValue::compound(vec![("keepAliveId", id)])).await?;
            }
            "ping" => {
                if let Some(id) = params.get("id").cloned() {
                    self.client.write("pong", PValue::compound(vec![("id", id)])).await?;
                }
            }
            "player_position" => {
                self.handle_position(params).await?;
                return Ok(Some(BotEvent::ForcedMove));
            }
            "set_health" => {
                if let Some(ev) = self.handle_health(params) {
                    return Ok(Some(ev));
                }
                return Ok(Some(BotEvent::Health));
            }
            "set_time" => self.handle_time(params),
            "set_default_spawn_position" => {
                if let Some(loc) = params.get("location") {
                    self.spawn_point = vec3(
                        loc.get("x").and_then(PValue::as_f64).unwrap_or(0.0),
                        loc.get("y").and_then(PValue::as_f64).unwrap_or(0.0),
                        loc.get("z").and_then(PValue::as_f64).unwrap_or(0.0),
                    );
                }
            }
            "level_chunk_with_light" => {
                match self.handle_chunk(params) {
                    Some((cx, cz)) => {
                        if std::env::var("CHUNK_DEBUG").is_ok() {
                            eprintln!("    CHUNK loaded ({cx},{cz}) total={}", self.world.loaded_chunk_count());
                        }
                        return Ok(Some(BotEvent::ChunkLoad(cx, cz)));
                    }
                    None => {
                        if std::env::var("CHUNK_DEBUG").is_ok() {
                            eprintln!("    CHUNK DROPPED (parse failed)");
                        }
                    }
                }
            }
            "chunk_batch_finished" => {
                if std::env::var("CHUNK_DEBUG").is_ok() {
                    eprintln!("    CHUNK batch_finished -> ack");
                }
                self.client
                    .write("chunk_batch_received", PValue::compound(vec![("chunksPerTick", PValue::num(20.0))]))
                    .await?;
            }
            "forget_level_chunk" => {
                let cx = params.get("chunkX").and_then(PValue::as_i32).unwrap_or(0);
                let cz = params.get("chunkZ").and_then(PValue::as_i32).unwrap_or(0);
                let _ = self.world.unload_column(cx, cz);
            }
            // A batch of block changes within one chunk section. Fluid-triggered
            // changes (e.g. lava→obsidian when water reaches it) arrive HERE, not as
            // individual block_update packets — without handling this the bot never
            // sees them (the obsidian a cast just made stays "lava" locally forever).
            "section_blocks_update" => {
                let coords = params.get("chunkCoordinates");
                let sx = coords.and_then(|c| c.get("x")).and_then(PValue::as_i64).unwrap_or(0);
                let sy = coords.and_then(|c| c.get("y")).and_then(PValue::as_i64).unwrap_or(0);
                let sz = coords.and_then(|c| c.get("z")).and_then(PValue::as_i64).unwrap_or(0);
                let sniff = std::env::var("CAST_SNIFF").is_ok();
                let mut last = None;
                if let Some(records) = params.get("records").and_then(PValue::as_list) {
                    for rec in records {
                        let v = rec.as_i64().unwrap_or(0) as u64;
                        let state = (v >> 12) as u32;
                        let p = v & 0xFFF;
                        let lx = ((p >> 8) & 0xF) as i64;
                        let lz = ((p >> 4) & 0xF) as i64;
                        let ly = (p & 0xF) as i64;
                        let (wx, wy, wz) = (sx * 16 + lx, sy * 16 + ly, sz * 16 + lz);
                        if sniff {
                            let nm = self.registry.blocks_by_state_id.get(&state).map(|b| b.name.clone()).unwrap_or_else(|| "?".into());
                            eprintln!("    SNIFF section_update ({wx},{wy},{wz}) -> {nm}");
                        }
                        self.world.set_block_state_id(vec3(wx as f64, wy as f64, wz as f64), state);
                        last = Some((wx as i32, wy as i32, wz as i32));
                    }
                }
                self.world.take_events();
                if let Some((x, y, z)) = last {
                    return Ok(Some(BotEvent::BlockUpdate(x, y, z)));
                }
            }
            "block_update" => {
                if let Some(loc) = params.get("location") {
                    let (x, y, z) = loc_xyz(loc);
                    let state = params.get("type").and_then(PValue::as_i32).unwrap_or(0) as u32;
                    if std::env::var("CAST_SNIFF").is_ok() {
                        let nm = self.registry.blocks_by_state_id.get(&state).map(|b| b.name.clone()).unwrap_or_else(|| "?".into());
                        eprintln!("    SNIFF block_update ({x},{y},{z}) -> {nm}");
                    }
                    self.world.set_block_state_id(vec3(x as f64, y as f64, z as f64), state);
                    self.world.take_events();
                    return Ok(Some(BotEvent::BlockUpdate(x, y, z)));
                }
            }
            "explode" => {
                self.explosions += 1;
            }
            "set_held_slot" => {
                if let Some(s) = params.get("slot").and_then(PValue::as_i32) {
                    self.held_slot = s;
                }
            }
            "container_set_content" => {
                self.handle_inventory_content(params);
                return Ok(Some(BotEvent::Inventory));
            }
            "container_set_slot" => {
                self.handle_inventory_slot(params);
            }
            "set_player_inventory" => {
                if let (Some(slot), Some(item)) =
                    (params.get("slotId").and_then(PValue::as_i32), params.get("contents"))
                {
                    // `slotId` is a PLAYER-INVENTORY index (hotbar 0–8, main 9–35, armour 36–39
                    // feet→head, offhand 40), not a window-0 index (result 0, 2×2 grid 1–4, armour 5–8,
                    // main 9–35, hotbar 36–44, offhand 45). Writing it raw put hotbar items into the craft
                    // result/grid slots and never updated the real hotbar slot, so crafted planks
                    // "vanished" and the crafting-table loop held race bots for hours (i4 44 min, i5 3 h).
                    let i = match slot {
                        0..=8 => slot as usize + 36,
                        9..=35 => slot as usize,
                        36..=39 => (44 - slot) as usize, // 36 feet → 8, 39 head → 5
                        40 => 45,
                        _ => usize::MAX,
                    };
                    if i < self.inventory.slots.len() {
                        self.inventory.slots[i] = from_notch(self.registry, item);
                    }
                }
            }
            "open_screen" => {
                self.handle_open_screen(params);
            }
            "container_close" => {
                self.sync_window_to_inventory();
                self.current_window = None;
            }
            "add_entity" => {
                let id = params.get("entityId").and_then(PValue::as_i32).unwrap_or(0);
                let mut e = Entity::new(id);
                e.position = vec3(
                    params.get("x").and_then(PValue::as_f64).unwrap_or(0.0),
                    params.get("y").and_then(PValue::as_f64).unwrap_or(0.0),
                    params.get("z").and_then(PValue::as_f64).unwrap_or(0.0),
                );
                e.entity_type = params.get("type").and_then(PValue::as_i32);
                if let Some(reg_id) = e.entity_type {
                    e.init(self.registry, reg_id);
                }
                self.entities.insert(id, e);
                return Ok(Some(BotEvent::EntitySpawn(id)));
            }
            "move_entity_pos" | "move_entity_pos_rot" => {
                let id = params.get("entityId").and_then(PValue::as_i32).unwrap_or(0);
                if let Some(e) = self.entities.get_mut(&id) {
                    // The schema names these dX/dY/dZ (protocol-schema.json packet_move_entity_pos). Reading
                    // "dx" got nothing, so every relative move was applied as 0, and entities only updated
                    // on rare absolute syncs. The cycle-4 dragon gym saw the dragon frozen at its first
                    // sighting while the server had it 40 blocks away. Old keys kept as a fallback.
                    let g = |k: &str, alt: &str| params.get(k).or_else(|| params.get(alt)).and_then(PValue::as_f64).unwrap_or(0.0) / 4096.0;
                    e.position = e.position.offset(g("dX", "dx"), g("dY", "dy"), g("dZ", "dz"));
                }
            }
            "entity_position_sync" | "teleport_entity" => {
                let id = params.get("entityId").and_then(PValue::as_i32).unwrap_or(0);
                if let Some(e) = self.entities.get_mut(&id) {
                    let g = |k: &str| params.get(k).and_then(PValue::as_f64);
                    if let (Some(x), Some(y), Some(z)) = (g("x"), g("y"), g("z")) {
                        e.position = vec3(x, y, z);
                    }
                }
            }
            "remove_entities" => {
                if let Some(ids) = params.get("entityIds").and_then(PValue::as_list) {
                    for id in ids {
                        if let Some(id) = id.as_i32() {
                            self.entities.remove(&id);
                        }
                    }
                }
            }
            "disconnect" => {
                // The reason is an NBT text component on 1.20.3+; log it raw either way.
                let reason = params
                    .get("reason")
                    .and_then(PValue::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{:?}", params.get("reason")));
                eprintln!("[bot] KICKED by server: {reason}");
                return Ok(Some(BotEvent::Kicked(reason)));
            }
            _ => return Ok(Some(BotEvent::Packet(name.to_string()))),
        }
        Ok(None)
    }

    async fn handle_login(&mut self, params: &PValue) -> std::io::Result<()> {
        self.entity.id = params.get("entityId").and_then(PValue::as_i32).unwrap_or(0);
        self.game.max_players = params.get("maxPlayers").and_then(PValue::as_i32).unwrap_or(0);
        self.game.hardcore = params.get("isHardcore").and_then(PValue::as_bool).unwrap_or(false);
        // Game mode lives in the per-world spawn info (1.20.5+); fall back to a
        // top-level field on older protocols. 0=survival 1=creative 2=adventure 3=spectator.
        let gm = params
            .get("worldState")
            .and_then(|w| w.get("gamemode"))
            .or_else(|| params.get("gamemode"))
            .or_else(|| params.get("gameMode"))
            .and_then(PValue::as_i32);
        if std::env::var("GM_DEBUG").is_ok() {
            eprintln!("LOGIN gamemode={gm:?} worldState={:?}", params.get("worldState"));
        }
        self.game.game_mode = match gm {
            Some(1) => "creative".into(),
            Some(2) => "adventure".into(),
            Some(3) => "spectator".into(),
            _ => "survival".into(),
        };
        // Initial dimension comes from the login packet's world SpawnInfo (the same
        // `name` field a `respawn` carries on a dimension change).
        if let Some(dim) = params
            .get("worldState")
            .and_then(|w| w.get("name"))
            .and_then(PValue::as_str)
        {
            self.game.dimension = dim.to_string();
            // Height for the LOGIN dimension too, not only on respawn: a bot that logs in already
            // in the nether kept the overworld's min_y -64 and read every nether block 64 too low
            // (harness 08:52: fortress brick "at (4,-3,147)", RCON: nether_bricks at (4,61,147)).
            let (min_y, height) = if dim.contains("nether") || dim.contains("the_end") { (0, 256) } else { (-64, 384) };
            self.game.min_y = min_y;
            self.game.height = height;
        }

        let mut brand_data = Vec::new();
        crate::varint::push_var_int(&mut brand_data, self.brand.len() as i32);
        brand_data.extend_from_slice(self.brand.as_bytes());
        self.client
            .write(
                "custom_payload",
                PValue::compound(vec![("channel", PValue::str("minecraft:brand")), ("data", PValue::Bytes(brand_data))]),
            )
            .await?;
        self.send_settings().await?;
        if self.client.protocol_version >= 769 {
            let _ = self.client.write("player_loaded", PValue::compound(vec![])).await;
        }
        Ok(())
    }

    async fn send_settings(&mut self) -> std::io::Result<()> {
        self.client
            .write(
                "client_information",
                PValue::compound(vec![
                    ("locale", PValue::str("en_US")),
                    ("viewDistance", PValue::num(8.0)),
                    ("chatFlags", PValue::num(0.0)),
                    ("chatColors", PValue::Bool(true)),
                    ("skinParts", PValue::num(127.0)),
                    ("mainHand", PValue::num(1.0)),
                    ("enableTextFiltering", PValue::Bool(false)),
                    ("enableServerListing", PValue::Bool(true)),
                    ("particleStatus", PValue::str("all")),
                ]),
            )
            .await
    }

    async fn handle_position(&mut self, params: &PValue) -> std::io::Result<()> {
        let flags = params.get("flags");
        let flag = |k: &str| flags.and_then(|f| f.get(k)).and_then(PValue::as_bool).unwrap_or(false);
        let g = |k: &str| params.get(k).and_then(PValue::as_f64).unwrap_or(0.0);

        let p = self.entity.position;
        self.entity.position = vec3(
            if flag("x") { p.x + g("x") } else { g("x") },
            if flag("y") { p.y + g("y") } else { g("y") },
            if flag("z") { p.z + g("z") } else { g("z") },
        );
        // Server position corrections were invisible: the water_cave_iron drowning (regression 3) sat
        // frozen at one position for ~100 s with jump held, and nothing said whether the server was
        // snapping it back. Log small corrections (respawns/tps are large and logged elsewhere).
        let q = self.entity.position;
        let moved = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2) + (q.z - p.z).powi(2)).sqrt();
        if moved < 4.0 {
            eprintln!("    [pos] server correction ({:.2},{:.2},{:.2}) → ({:.2},{:.2},{:.2})", p.x, p.y, p.z, q.x, q.y, q.z);
        }
        let yaw = from_notchian_yaw(g("yaw"));
        let pitch = from_notchian_pitch(g("pitch"));
        self.entity.yaw = if flag("yaw") { self.entity.yaw + yaw } else { yaw };
        self.entity.pitch = if flag("pitch") { self.entity.pitch + pitch } else { pitch };
        self.entity.velocity = crate::vec3::ZERO;

        if let Some(id) = params.get("teleportId").cloned() {
            self.client.write("accept_teleportation", PValue::compound(vec![("teleportId", id)])).await?;
            self.last_teleport = Instant::now();
        }
        self.should_physics = true;
        self.last_sent = None;
        Ok(())
    }

    fn handle_health(&mut self, params: &PValue) -> Option<BotEvent> {
        self.health = params.get("health").and_then(PValue::as_f64).unwrap_or(self.health);
        self.food = params.get("food").and_then(PValue::as_f64).unwrap_or(self.food);
        self.food_saturation = params.get("foodSaturation").and_then(PValue::as_f64).unwrap_or(self.food_saturation);
        self.entity.health = self.health;
        if !self.spawned && self.health > 0.0 {
            self.spawned = true;
            self.alive = true;
            return Some(BotEvent::Spawn);
        }
        if self.health <= 0.0 && self.alive {
            self.alive = false;
            // Counted HERE (the protocol truth), not by callers polling `alive`: steps and the
            // survival escape respawn internally, so the gym loop saw deaths=0 while the server
            // logged `rust-gym-002 drowned` twice (cycle-2 water gym).
            self.deaths += 1;
            DEATH_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Some(BotEvent::Death);
        }
        if self.health > 0.0 && !self.alive {
            self.alive = true;
            return Some(BotEvent::Spawn);
        }
        None
    }

    fn handle_time(&mut self, params: &PValue) {
        let age = params.get("gameTime").and_then(PValue::as_i64).unwrap_or(self.time.age);
        let tod = params
            .get("timeOfDay")
            .and_then(PValue::as_i64)
            .or_else(|| {
                params
                    .get("clocks")
                    .and_then(PValue::as_list)
                    .and_then(|c| c.first())
                    .and_then(|c| c.get("time"))
                    .and_then(PValue::as_i64)
            })
            .map(|t| t.abs())
            .unwrap_or(self.time.time_of_day);
        self.time = TimeInfo { age, time_of_day: tod % 24000, day: tod / 24000, is_day: tod % 24000 < 13000 };
    }

    fn handle_chunk(&mut self, params: &PValue) -> Option<(i32, i32)> {
        let cx = params.get("x").and_then(PValue::as_i32)?;
        let cz = params.get("z").and_then(PValue::as_i32)?;
        let data = params.get("chunkData").and_then(PValue::as_bytes)?.to_vec();
        let min_y = self.game.min_y;
        let height = self.game.height;
        let no_array_length = self.client.protocol_version >= 770;
        let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut col = ChunkColumn::new(ChunkColumnOptions {
                min_y: Some(min_y),
                world_height: Some(height),
                max_bits_per_block: GLOBAL_BITS_PER_BLOCK,
                max_bits_per_biome: GLOBAL_BITS_PER_BIOME,
            });
            col.load(&data, no_array_length);
            col
        }));
        match loaded {
            Ok(col) => {
                self.world.set_column(cx, cz, col);
                self.world.take_events();
                Some((cx, cz))
            }
            Err(_) => None,
        }
    }

    /// Choose which window a packet's `windowId` targets.
    fn window_for(&mut self, window_id: i32) -> Option<&mut Window> {
        if window_id == 0 || window_id == -1 {
            Some(&mut self.inventory)
        } else if self.current_window.as_ref().map(|w| w.id) == Some(window_id) {
            self.current_window.as_mut()
        } else {
            None
        }
    }

    fn handle_inventory_content(&mut self, params: &PValue) {
        self.inv_revision = self.inv_revision.wrapping_add(1);
        let window_id = params.get("windowId").and_then(PValue::as_i32).unwrap_or(-1);
        let state_id = params.get("stateId").and_then(PValue::as_i32);
        let registry = self.registry;
        // Verbatim, grid included (decision 7). The typecraft port moved 2×2 grid items into empty
        // inventory slots here on the client only: the server still held them in the grid, the craft
        // grabbed phantoms and made oak_buttons (tests/fixtures/stale-craft-window.json).
        if let Some(items) = params.get("items").and_then(PValue::as_list) {
            let items = items.iter().map(|s| from_notch(registry, s)).collect();
            crate::window::apply_server_content(&mut self.inventory, self.current_window.as_mut(), window_id, items);
        }
        if let (Some(sid), Some(window)) = (state_id, self.window_for(window_id)) {
            window.state_id = sid;
        }
    }

    fn handle_inventory_slot(&mut self, params: &PValue) {
        self.inv_revision = self.inv_revision.wrapping_add(1);
        let window_id = params.get("windowId").and_then(PValue::as_i32).unwrap_or(-1);
        let state_id = params.get("stateId").and_then(PValue::as_i32);
        let registry = self.registry;
        let Some(slot) = params.get("slot").and_then(PValue::as_i32) else {
            return;
        };
        let item = params.get("item").and_then(|it| from_notch(registry, it));
        crate::window::apply_server_slot(&mut self.inventory, self.current_window.as_mut(), window_id, slot, item);
        if let (Some(sid), Some(window)) = (state_id, self.window_for(window_id)) {
            window.state_id = sid;
        }
    }

    fn handle_open_screen(&mut self, params: &PValue) {
        let window_id = params.get("windowId").and_then(PValue::as_i32).unwrap_or(0);
        let type_id = params.get("inventoryType").and_then(PValue::as_i64).unwrap_or(0);
        let title = params.get("windowTitle").and_then(PValue::as_str).unwrap_or("").to_string();
        let mut win =
            crate::window::create_window_from_type(self.registry, window_id, type_id, None, &title, None);
        // Seed the new window's inventory portion with our current inventory — the
        // client already knows its items; without this the container opens "empty",
        // crafts can't find ingredients, and closing wipes the real inventory.
        if let Some(w) = win.as_mut() {
            let inv_len = w.inventory_end - w.inventory_start;
            for i in 0..inv_len {
                let ws = w.inventory_start + i;
                let ps = self.inventory.inventory_start + i;
                if ws < w.slots.len() && ps < self.inventory.slots.len() {
                    w.slots[ws] = self.inventory.slots[ps].clone();
                }
            }
        }
        self.current_window = win;
    }

    /// Copy a closing container's inventory section back into the player inventory.
    fn sync_window_to_inventory(&mut self) {
        if let Some(w) = self.current_window.take() {
            let inv_len = w.inventory_end - w.inventory_start;
            for i in 0..inv_len {
                let cs = w.inventory_start + i;
                let ps = self.inventory.inventory_start + i;
                if cs < w.slots.len() && ps < self.inventory.slots.len() {
                    self.inventory.slots[ps] = w.slots[cs].clone();
                }
            }
        }
    }

    // ── World / block queries ──

    pub fn block_state_at(&self, x: i32, y: i32, z: i32) -> u32 {
        self.world.get_block_state_id(vec3(x as f64, y as f64, z as f64)).unwrap_or(0)
    }

    /// Block name + properties at a world coordinate (`None` if unloaded/air).
    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Option<BlockInfo> {
        let state = self.world.get_block_state_id(vec3(x as f64, y as f64, z as f64))?;
        if state == 0 || !self.registry.blocks_by_state_id.contains_key(&state) {
            return None;
        }
        Some(state_id_to_block(self.registry, state))
    }

    /// Cheap per-tick pose/time update for the live viewer — just writes the camera pose
    /// under the shared lock (no world scan), so the streamed feed stays smooth at tick rate.
    fn update_viewer_pose(&self) {
        let Some(handle) = &self.viewer else {
            return;
        };
        let p = self.entity.position;
        let mut shared = handle.lock().unwrap();
        shared.pose = (p.x, p.y, p.z, self.entity.yaw, self.entity.pitch);
        shared.min_y = self.game.min_y;
        shared.height = self.game.height;
        shared.time = self.time.time_of_day;
    }

    /// Fill the live-3D-viewer snapshot: update pose/time, and dump a bounded number
    /// of newly-loaded near chunks into the shared buffer for the SSE server to stream.
    /// Bounded work per call (≤4 columns) + the throttle keep it off the craft path.
    fn update_viewer(&mut self) {
        let Some(handle) = self.viewer.clone() else {
            return;
        };
        let p = self.entity.position;
        let (bcx, bcz) = ((p.x.floor() as i32) >> 4, (p.z.floor() as i32) >> 4);
        const R: i32 = 6;
        // Which near columns are loaded in the world but not yet dumped into the snapshot?
        let missing: Vec<(i32, i32)> = {
            let shared = handle.lock().unwrap();
            let mut out = Vec::new();
            for cx in (bcx - R)..=(bcx + R) {
                for cz in (bcz - R)..=(bcz + R) {
                    if !shared.chunks.contains_key(&(cx, cz)) {
                        out.push((cx, cz));
                    }
                }
            }
            out
        };
        // Dump at most a handful per call (dumping a full column is non-trivial work;
        // this spreads the view-distance fill over several ticks like a chunk-load queue).
        let dumps: Vec<((i32, i32), Vec<u8>)> = missing
            .into_iter()
            .filter_map(|(cx, cz)| {
                self.world
                    .get_loaded_column(cx, cz)
                    .map(|col| ((cx, cz), col.dump(true)))
            })
            .take(4)
            .collect();
        let mut shared = handle.lock().unwrap();
        shared.pose = (p.x, p.y, p.z, self.entity.yaw, self.entity.pitch);
        shared.min_y = self.game.min_y;
        shared.height = self.game.height;
        shared.time = self.time.time_of_day;
        if !dumps.is_empty() {
            for (key, bytes) in dumps {
                shared.chunks.insert(key, bytes);
            }
            shared.seq += 1;
        }
    }

    /// Write a top-down terrain slice around the bot to /tmp/rust-view-<name>.txt for the
    /// dashboard: for each column near the bot, the topmost solid block's category char.
    /// Cheap (early-exits on the first solid block per column), gated by RUST_VIEW.
    #[allow(dead_code)]
    fn export_view(&self) {
        let p = self.entity.position;
        let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        const R: i32 = 20;
        let cat = |name: &str| -> char {
            if name.contains("water") {
                '~'
            } else if name.contains("lava") {
                '!'
            } else if name.ends_with("_log") || name.ends_with("_wood") {
                'T'
            } else if name.ends_with("_leaves") || name.contains("grass") || name.contains("flower") || name.contains("fern") {
                'g'
            } else if name.ends_with("_ore") {
                'o'
            } else if name == "sand" || name.contains("sandstone") {
                'S'
            } else if name == "gravel" {
                'v'
            } else if name == "obsidian" {
                'X'
            } else if name.contains("portal") {
                'P'
            } else if name == "crafting_table" || name == "furnace" {
                'C'
            } else if name == "dirt" || name == "coarse_dirt" {
                'd'
            } else {
                '.' // stone/deepslate/other solid
            }
        };
        let mut out = String::with_capacity(((2 * R + 1) * (2 * R + 2)) as usize + 64);
        out.push_str(&format!("{} {} {} {} {:.0}\n", self.username(), bx, by, bz, self.entity.yaw));
        for dz in -R..=R {
            for dx in -R..=R {
                let mut ch = ' ';
                for y in (by - 12..=by + 8).rev() {
                    if let Some(b) = self.block_at(bx + dx, y, bz + dz) {
                        let n = b.name;
                        if n == "air" || n == "cave_air" || n == "void_air" {
                            continue;
                        }
                        ch = cat(&n);
                        break;
                    }
                }
                out.push(ch);
            }
            out.push('\n');
        }
        let _ = std::fs::write(format!("/tmp/rust-view-{}.txt", self.username()), out);
    }

    /// Find up to `count` blocks matching `name` within `max_distance`,
    /// nearest first, requiring line-of-sight + an exposed face.
    pub fn find_blocks(&self, name: &str, max_distance: i32, count: usize) -> Vec<(i32, i32, i32)> {
        let Some(def) = self.registry.blocks_by_name.get(name) else {
            return vec![];
        };
        let target_id = def.id;
        let origin = self.entity.position;
        let (ox, oy, oz) = (origin.x.floor() as i32, origin.y.floor() as i32, origin.z.floor() as i32);
        let mut results = Vec::new();
        for dist in 0..=max_distance {
            for dx in -dist..=dist {
                for dy in -dist..=dist {
                    for dz in -dist..=dist {
                        if dx.abs() != dist && dy.abs() != dist && dz.abs() != dist {
                            continue;
                        }
                        if dx * dx + dy * dy + dz * dz > max_distance * max_distance {
                            continue;
                        }
                        let (x, y, z) = (ox + dx, oy + dy, oz + dz);
                        let Some(state) = self.world.get_block_state_id(vec3(x as f64, y as f64, z as f64)) else {
                            continue;
                        };
                        if state == 0 {
                            continue;
                        }
                        let matches = self.registry.blocks_by_state_id.get(&state).map(|d| d.id) == Some(target_id);
                        if matches && self.is_exposed(x, y, z) && self.can_see_block(x, y, z) {
                            results.push((x, y, z));
                            if results.len() >= count {
                                return results;
                            }
                        }
                    }
                }
            }
        }
        results
    }

    pub fn find_block(&self, name: &str, max_distance: i32) -> Option<(i32, i32, i32)> {
        self.find_blocks(name, max_distance, 1).into_iter().next()
    }

    /// Like `find_blocks` but requires only an EXPOSED face (an air/transparent
    /// neighbour), not strict line-of-sight. Lava/water at or below floor level is
    /// invisible to the LOS raycast from a standing bot (the sightline grazes the
    /// floor), yet it's a perfectly valid pour/fill target the bot can walk to. Use
    /// this for locating fluid pools — NOT for ore (that would be X-ray).
    /// The nearest (horizontally) EXPOSED block with one of `names`, anywhere in the LOADED chunks
    /// and within `y_range` (inclusive). Same honesty rule as `find_exposed_blocks` / `blockSeen`:
    /// only blocks touching air count, so buried ore/lava stays invisible (no X-ray) — but the
    /// search covers the whole view distance (±96 blocks at view-distance 6) instead of a small
    /// radius. Cheap: a palette check per 16³ section (`ChunkSection::contains_where`) and a cell
    /// scan only inside sections that hold one of the states.
    /// Record every lava SOURCE in the loaded chunks that has an AIR face (air, cave_air, void_air, or a
    /// see-through non-fluid block such as a torch) into `lava_seen`. Lava touching only stone, lava or
    /// an unloaded cell is never recorded: the bot cannot see it (cycle 6, decision 5).
    pub fn record_lava_sightings(&mut self) {
        let Some(def) = self.registry.blocks_by_name.get("lava") else { return };
        let source = def.min_state_id;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let mut found: Vec<(i32, i32, i32)> = Vec::new();
        for (&(cx, cz), col) in self.world.columns.iter() {
            for (si, sec) in col.sections.iter().enumerate() {
                if !sec.contains_where(&|s: u32| s == source) {
                    continue;
                }
                let y0 = col.min_y + si as i32 * 16;
                for y in 0..16 {
                    for z in 0..16 {
                        for x in 0..16 {
                            if sec.get_block(x, y, z) == source {
                                found.push((cx * 16 + x as i32, y0 + y as i32, cz * 16 + z as i32));
                            }
                        }
                    }
                }
            }
        }
        for c in found {
            if !self.lava_seen.contains_key(&c) && self.is_exposed(c.0, c.1, c.2) {
                self.lava_seen.insert(c, now);
            }
        }
    }

    /// Was this cell ever seen as an exposed lava source (or is it one now)? The honest-lava check.
    pub fn lava_was_seen(&self, c: (i32, i32, i32)) -> bool {
        self.lava_seen.contains_key(&c) || (self.block_at(c.0, c.1, c.2).is_some_and(|b| b.name == "lava") && self.is_exposed(c.0, c.1, c.2))
    }

    /// DEBUG ASSERTION (cycle 6, decision 5), on in gyms (`GYM` set): a lava-site decision whose target
    /// cell was never seen exposed panics, so no path can quietly read lava through rock again.
    pub fn assert_lava_seen(&self, c: (i32, i32, i32), ctx: &str) {
        if std::env::var("GYM").is_ok_and(|v| !v.is_empty()) && !self.lava_was_seen(c) {
            panic!("X-RAY ASSERTION: lava-site decision '{ctx}' targets {c:?}, a cell never seen with an air face");
        }
    }

    /// The largest remembered cluster of SEEN lava: among remembered sources within `horiz` blocks
    /// horizontally and inside `y_range`, the one with the most remembered sources within 6 blocks (3D);
    /// distance breaks ties. Sources still loaded and no longer lava (scooped, turned to obsidian) are
    /// dropped. Returns (source, count) when the count is at least `min_count`.
    pub fn lava_seen_cluster(&self, y_range: (i32, i32), min_count: usize, horiz: f64, skip: &dyn Fn((i32, i32, i32)) -> bool) -> Option<((i32, i32, i32), usize)> {
        let p = self.entity.position;
        let live: Vec<(i32, i32, i32)> = self
            .lava_seen
            .keys()
            .copied()
            .filter(|c| self.block_at(c.0, c.1, c.2).is_none_or(|b| b.name == "lava"))
            .collect();
        let mut grid: HashMap<(i32, i32, i32), Vec<(i32, i32, i32)>> = HashMap::new();
        for &s in &live {
            grid.entry((s.0.div_euclid(4), s.1.div_euclid(4), s.2.div_euclid(4))).or_default().push(s);
        }
        let mut best: Option<((i32, i32, i32), usize, f64)> = None;
        for &c in &live {
            if c.1 < y_range.0 || c.1 > y_range.1 || skip(c) {
                continue;
            }
            let d = ((c.0 as f64 + 0.5 - p.x).powi(2) + (c.2 as f64 + 0.5 - p.z).powi(2)).sqrt();
            if d > horiz {
                continue;
            }
            let (gx, gy, gz) = (c.0.div_euclid(4), c.1.div_euclid(4), c.2.div_euclid(4));
            let mut n = 0usize;
            for dx in -2..=2 {
                for dy in -2..=2 {
                    for dz in -2..=2 {
                        if let Some(v) = grid.get(&(gx + dx, gy + dy, gz + dz)) {
                            n += v.iter().filter(|s| (s.0 - c.0).pow(2) + (s.1 - c.1).pow(2) + (s.2 - c.2).pow(2) <= 36).count();
                        }
                    }
                }
            }
            if n >= min_count && best.is_none_or(|(_, bn, bd)| n > bn || (n == bn && d < bd)) {
                best = Some((c, n, d));
            }
        }
        best.map(|(c, n, _)| (c, n))
    }

    pub fn find_loaded_nearest(&self, names: &[&str], y_range: (i32, i32)) -> Option<(i32, i32, i32)> {
        let ranges: Vec<(u32, u32)> = names
            .iter()
            .filter_map(|n| self.registry.blocks_by_name.get(*n))
            .map(|d| (d.min_state_id, d.max_state_id))
            .collect();
        if ranges.is_empty() {
            return None;
        }
        let pred = |s: u32| ranges.iter().any(|&(lo, hi)| s >= lo && s <= hi);
        let p = self.entity.position;
        let mut best: Option<((i32, i32, i32), f64)> = None;
        for (&(cx, cz), col) in self.world.columns.iter() {
            // A chunk whose centre is farther than the best hit (minus half a diagonal) can't win.
            let cd = (((cx * 16 + 8) as f64 - p.x).powi(2) + ((cz * 16 + 8) as f64 - p.z).powi(2)).sqrt();
            if best.is_some_and(|(_, bd)| cd - 12.0 > bd) {
                continue;
            }
            for (si, sec) in col.sections.iter().enumerate() {
                let y0 = col.min_y + si as i32 * 16;
                if y0 + 15 < y_range.0 || y0 > y_range.1 || !sec.contains_where(&pred) {
                    continue;
                }
                for y in 0..16 {
                    let wy = y0 + y as i32;
                    if wy < y_range.0 || wy > y_range.1 {
                        continue;
                    }
                    for z in 0..16 {
                        for x in 0..16 {
                            if !pred(sec.get_block(x, y, z)) {
                                continue;
                            }
                            let w = (cx * 16 + x as i32, wy, cz * 16 + z as i32);
                            let d = ((w.0 as f64 + 0.5 - p.x).powi(2) + (w.2 as f64 + 0.5 - p.z).powi(2)).sqrt();
                            if best.is_none_or(|(_, bd)| d < bd) && self.is_exposed(w.0, w.1, w.2) {
                                best = Some((w, d));
                            }
                        }
                    }
                }
            }
        }
        best.map(|(w, _)| w)
    }

    pub fn find_exposed_blocks(&self, name: &str, max_distance: i32, count: usize) -> Vec<(i32, i32, i32)> {
        let Some(def) = self.registry.blocks_by_name.get(name) else {
            return vec![];
        };
        let target_id = def.id;
        let origin = self.entity.position;
        let (ox, oy, oz) = (origin.x.floor() as i32, origin.y.floor() as i32, origin.z.floor() as i32);
        let mut results = Vec::new();
        for dist in 0..=max_distance {
            for dx in -dist..=dist {
                for dy in -dist..=dist {
                    for dz in -dist..=dist {
                        if dx.abs() != dist && dy.abs() != dist && dz.abs() != dist {
                            continue;
                        }
                        if dx * dx + dy * dy + dz * dz > max_distance * max_distance {
                            continue;
                        }
                        let (x, y, z) = (ox + dx, oy + dy, oz + dz);
                        let Some(state) = self.world.get_block_state_id(vec3(x as f64, y as f64, z as f64)) else {
                            continue;
                        };
                        if state == 0 {
                            continue;
                        }
                        let matches = self.registry.blocks_by_state_id.get(&state).map(|d| d.id) == Some(target_id);
                        if matches && self.is_exposed(x, y, z) {
                            results.push((x, y, z));
                            if results.len() >= count {
                                return results;
                            }
                        }
                    }
                }
            }
        }
        results
    }

    /// A face the bot could see this block through: air (any kind), water, or a see-through non-fluid block.
    /// Not lava (the registry marks lava `transparent`, so the old test counted every source inside a buried
    /// lava lake as exposed, cycle 6 audit), and not an unloaded cell (nothing is known there).
    pub fn is_exposed(&self, x: i32, y: i32, z: i32) -> bool {
        for (ox, oy, oz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            let Some(s) = self.world.get_block_state_id(vec3((x + ox) as f64, (y + oy) as f64, (z + oz) as f64)) else {
                continue;
            };
            if s == 0 {
                return true;
            }
            if let Some(d) = self.registry.blocks_by_state_id.get(&s) {
                if d.name != "lava" && (d.transparent || d.name.ends_with("air")) {
                    return true;
                }
            }
        }
        false
    }

    pub fn can_see_block(&self, x: i32, y: i32, z: i32) -> bool {
        let eye = vec3(self.entity.position.x, self.entity.position.y + PLAYER_EYE_HEIGHT, self.entity.position.z);
        let center = vec3(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5);
        let d = center.subtract(eye);
        let dist = d.length();
        if dist == 0.0 {
            return true;
        }
        let dir = d.scale(1.0 / dist);
        match raycast(&self.world, eye, dir, dist + 1.0, None) {
            None => true,
            Some(hit) => {
                hit.position.x.floor() as i32 == x
                    && hit.position.y.floor() as i32 == y
                    && hit.position.z.floor() as i32 == z
            }
        }
    }

    // ── Controls / look ──

    pub fn set_control_state(&mut self, control: &str, state: bool) {
        match control {
            "forward" => self.control_state.forward = state,
            "back" => self.control_state.back = state,
            "left" => self.control_state.left = state,
            "right" => self.control_state.right = state,
            "jump" => self.control_state.jump = state,
            "sprint" => self.control_state.sprint = state,
            "sneak" => self.control_state.sneak = state,
            _ => {}
        }
    }

    pub fn clear_control_states(&mut self) {
        self.control_state = ControlState::default();
    }

    /// Face a yaw/pitch (radians) immediately.
    pub fn look(&mut self, yaw: f64, pitch: f64) {
        self.entity.yaw = yaw;
        self.entity.pitch = pitch;
    }

    /// Face a world point immediately.
    pub fn look_at(&mut self, point: Vec3) {
        let eye = vec3(self.entity.position.x, self.entity.position.y + 1.62, self.entity.position.z);
        let delta = point.subtract(eye);
        let yaw = (-delta.x).atan2(-delta.z);
        let ground = (delta.x * delta.x + delta.z * delta.z).sqrt();
        let pitch = delta.y.atan2(ground);
        self.look(yaw, pitch);
    }

    // ── Actions ──

    /// Send a chat command (no leading slash).
    pub async fn run_command(&mut self, command: &str) -> std::io::Result<()> {
        self.client.write("chat_command", PValue::compound(vec![("command", PValue::str(command))])).await
    }

    pub async fn set_held_slot(&mut self, slot: i32) -> std::io::Result<()> {
        self.held_slot = slot;
        self.client.write("set_carried_item", PValue::compound(vec![("slotId", PValue::num(slot as f64))])).await
    }

    pub async fn swing_arm(&mut self) -> std::io::Result<()> {
        self.client.write("swing", PValue::compound(vec![("hand", PValue::num(0.0))])).await
    }

    /// Force the current position + LOOK to the server right now. `look_at` only updates the
    /// local rotation; the position packet that carries it goes out on the NEXT physics tick —
    /// after any use/place packet sent in between — so the server raycasts a STALE rotation.
    /// That is exactly why a bucket pour aimed into a cup landed on the bot's own feet (the
    /// server still held the pre-look pitch). Same trick `attack` uses for reach checks.
    /// `last_sent` is restored so the physics tick still re-sends normally.
    pub async fn sync_look(&mut self) -> std::io::Result<()> {
        let saved = self.last_sent.take();
        let r = self.send_position().await;
        self.last_sent = saved;
        r
    }

    /// Melee-attack an entity (left-click): face it, swing the arm, send the `attack` packet.
    pub async fn attack(&mut self, entity_id: i32) -> std::io::Result<()> {
        if let Some(e) = self.entities.get(&entity_id) {
            let c = e.position;
            self.look_at(vec3(c.x, c.y + 0.7, c.z));
        }
        // Sync our current position+look to the server BEFORE attacking. send_position otherwise
        // fires only on the next physics tick — AFTER the attack packet — so the server evaluates
        // the attack's reach/angle against a STALE pose and rejects it. `last_sent` is left
        // unchanged so the physics tick still re-sends.
        let saved = self.last_sent.take();
        let _ = self.send_position().await;
        self.last_sent = saved;
        self.swing_arm().await?;
        // 775 (26.1.2) split attacking into a dedicated `attack` packet (0x01) — just the target
        // entity id. The old `interact` packet (0x1a) is now right-click ONLY, so sending an
        // interact-with-mouse=1 was a no-op right-click on the mob (decoded fine, dealt no damage;
        // and without a `hand` it even failed to decode → kicked us). See minecraft-data pc/26.1.
        self.client
            .write("attack", PValue::compound(vec![("entityId", PValue::num(entity_id as f64))]))
            .await
    }

    /// Dig the block at (x,y,z): face it, send start, wait the break time while
    /// swinging, then send finish.
    pub async fn dig(&mut self, x: i32, y: i32, z: i32) -> std::io::Result<()> {
        site("dig");
        // Breath/water alarm: a task's own digging must stop too, or the step never returns —
        // the water_aquifer bot mined 10 cobble from inside the flooded chamber after its alarm.
        // The escape itself (leave_water) sets `escaping`, so its cap/bank digs still run.
        if self.breath_alarm && !self.escaping {
            return Ok(());
        }
        let center = vec3(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5);
        let face = self.dig_face(x, y, z);
        // Stop moving and let the server agree on where we are before digging.
        // While walking/climbing the client position can run ahead of the
        // server's (which periodically teleports us back); a dig sent from the
        // client's position is then rejected as out-of-reach. Settling on the
        // ground with no controls lets the positions reconcile.
        self.clear_control_states();
        // Briefly stop and settle on the ground so client & server agree on our
        // position before mining — a dig sent mid-move is rejected as
        // out-of-reach (the server periodically teleports us back).
        for _ in 0..12 {
            if matches!(self.drive_tick().await?, DriveStep::Disconnected) {
                return Ok(());
            }
            if self.entity.on_ground && self.entity.velocity.length() < 0.05 {
                break;
            }
        }
        // Face the block and push that look to the server before starting — the
        // server validates the player is looking at the block.
        self.look_at(center);
        self.wait_ticks(2).await?;
        let time = self.dig_time(x, y, z);
        self.sequence += 1;
        let seq = self.sequence;
        self.client
            .write(
                "player_action",
                PValue::compound(vec![
                    ("status", PValue::num(0.0)),
                    ("location", block_pos(x, y, z)),
                    ("face", PValue::num(face as f64)),
                    ("sequence", PValue::num(seq as f64)),
                ]),
            )
            .await?;
        self.swing_arm().await?;

        // Mine for the computed break time (like the vanilla client), swinging
        // periodically and holding the look on the block, then send STOP. A small
        // margin covers rounding. Break early if the server turns it to air.
        // Hold the dig ~35% past the client-computed break time (+200 ms). The server tracks its
        // own progress; a STOP that arrives before the server is done ABORTS the break (deepslate
        // frame cells stayed solid through two dig attempts on natural terrain), while a late
        // STOP is accepted. Breaking early via block_state_at==0 still exits as soon as it lands.
        let mine_for = if time.is_zero() { Duration::ZERO } else { time.mul_f64(1.35) + Duration::from_millis(200) };
        let deadline = Instant::now() + mine_for;
        let start_state = self.block_state_at(x, y, z);
        while Instant::now() < deadline {
            self.look_at(center); // hold the look on the block while mining
            self.swing_arm().await?;
            self.wait_ticks(2).await?;
            if self.block_state_at(x, y, z) == 0 {
                break;
            }
        }
        let _ = start_state;

        self.sequence += 1;
        let fseq = self.sequence;
        self.client
            .write(
                "player_action",
                PValue::compound(vec![
                    ("status", PValue::num(2.0)),
                    ("location", block_pos(x, y, z)),
                    ("face", PValue::num(face as f64)),
                    ("sequence", PValue::num(fseq as f64)),
                ]),
            )
            .await?;

        // The server breaks the block in response to FINISH (status 2). With the
        // 1.19+ prediction model it does NOT echo a block_update back to the
        // breaker, so wait a few ticks (in case it does), then count nearby item
        // drops — a drop proves the break landed even with no block_update.
        let pre = self.block_state_at(x, y, z);
        for _ in 0..6 {
            if matches!(self.drive_tick().await?, DriveStep::Disconnected) {
                break;
            }
            if self.block_state_at(x, y, z) == 0 {
                break;
            }
        }
        // 1.19+ block prediction: the server breaks the block in response to
        // FINISH but does NOT echo a block_update back to the breaking player, so
        // our world would stay stale and we'd re-dig the same block forever.
        // Reflect the break locally for blocks that break by hand in this time.
        if self.block_state_at(x, y, z) != 0 && time < Duration::from_secs(4) {
            self.world.set_block_state_id(vec3(x as f64, y as f64, z as f64), 0);
        }
        let item_type = self.registry.entities_by_name.get("item").map(|d| d.id);
        let drops = self
            .entities
            .values()
            .filter(|e| item_type.is_none() || e.entity_type == item_type)
            .filter(|e| {
                let dx = e.position.x - (x as f64 + 0.5);
                let dy = e.position.y - (y as f64 + 0.5);
                let dz = e.position.z - (z as f64 + 0.5);
                dx * dx + dy * dy + dz * dz < 9.0
            })
            .count();
        if std::env::var("DIG_DEBUG").is_ok() {
            let p = self.entity.position;
            let dist = ((x as f64 + 0.5 - p.x).powi(2) + (y as f64 + 0.5 - p.y - 1.62).powi(2) + (z as f64 + 0.5 - p.z).powi(2)).sqrt();
            eprintln!(
                "DIG ({x},{y},{z}) bot=({:.1},{:.1},{:.1}) eyeDist={dist:.1} see={} ground={} sinceTp={}ms preFinish={pre} broke={} dropsNear={drops} held={:?} time={}ms underwater={}",
                p.x, p.y, p.z, self.can_see_block(x, y, z), self.entity.on_ground, self.last_teleport.elapsed().as_millis(), self.block_state_at(x, y, z) == 0,
                self.held_item().map(|i| i.name.clone()), time.as_millis(), self.head_submerged()
            );
        }
        Ok(())
    }

    /// Dig toward (x,y,z): raycast from the eye and dig whatever solid block is
    /// in the way (e.g. a leaf occluding a trunk), or the target itself if the
    /// ray reaches it. Returns `true` if it dug the actual target. `false` if the
    /// target is out of reach. Call repeatedly to clear a path to a block.
    pub async fn dig_toward(&mut self, x: i32, y: i32, z: i32) -> std::io::Result<bool> {
        let eye = vec3(self.entity.position.x, self.entity.position.y + PLAYER_EYE_HEIGHT, self.entity.position.z);
        let center = vec3(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5);
        let d = center.subtract(eye);
        let dist = d.length();
        if dist > 5.5 {
            return Ok(false);
        }
        self.look_at(center);
        self.wait_ticks(2).await?;
        // The server validates line-of-sight, so an occluded block can't be dug —
        // we must clear the occluder first. Use the SAME raycast that
        // `can_see_block` uses and dig whatever it hits first: the target if the
        // line is clear, otherwise the occluding block (a leaf/trunk in the way).
        // Repeated calls tunnel a sight-line through to the trunk.
        let dir = d.scale(1.0 / dist.max(1e-6));
        match raycast(&self.world, eye, dir, dist + 1.0, None) {
            Some(hit) => {
                let (bx, by, bz) = (hit.position.x.floor() as i32, hit.position.y.floor() as i32, hit.position.z.floor() as i32);
                self.dig(bx, by, bz).await?;
                Ok((bx, by, bz) == (x, y, z))
            }
            None => {
                self.dig(x, y, z).await?;
                Ok(true)
            }
        }
    }

    /// The block face nearest the bot: 0 bottom, 1 top, 2 north(-z), 3 south(+z),
    /// 4 west(-x), 5 east(+x).
    fn dig_face(&self, x: i32, y: i32, z: i32) -> i32 {
        let eye = vec3(self.entity.position.x, self.entity.position.y + 1.62, self.entity.position.z);
        let d = eye.subtract(vec3(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5));
        let (ax, ay, az) = (d.x.abs(), d.y.abs(), d.z.abs());
        if ay >= ax && ay >= az {
            if d.y >= 0.0 { 1 } else { 0 }
        } else if ax >= az {
            if d.x >= 0.0 { 5 } else { 4 }
        } else if d.z >= 0.0 {
            3
        } else {
            2
        }
    }

    /// Estimated break time for the block at (x,y,z).
    pub fn dig_time(&self, x: i32, y: i32, z: i32) -> Duration {
        if self.game.game_mode == "creative" {
            return Duration::ZERO;
        }
        let state = self.block_state_at(x, y, z);
        let name = self.block_at(x, y, z).map(|b| b.name.clone()).unwrap_or_default();
        // `data/blocks.json` carries a PLACEHOLDER hardness (datagen writes 1.0 for every solid
        // block — the vanilla reports have no destroy time), so the computed break time was a
        // third of the real one for deepslate and the dig STOP landed on the server's 70 %
        // rejection threshold: natural deepslate cells/headroom failed their first dig about
        // half the time (locally predicted air, then the server's block_update put the
        // deepslate back). Use vanilla values by name; the registry is the fallback.
        let hardness = block_hardness(&name)
            .or_else(|| self.registry.blocks_by_state_id.get(&state).and_then(|d| d.hardness))
            .unwrap_or(1.0);
        if hardness <= 0.0 {
            return Duration::ZERO;
        }
        let tool = tool_speed(self.held_item().map(|i| i.name.as_str()), Some(name.clone()));
        let mut speed = tool;
        // Vanilla mining penalties the SDK was missing: eyes in water (no Aqua Affinity) ÷5, not
        // on the ground ÷5. Without them an underwater/floating dig's STOP went out 5–25× early,
        // the server rejected the break and the block came back — the capped-cave escape in the
        // cycle-2 water gym dug its cap forever and drowned.
        if self.head_submerged() {
            speed /= 5.0;
        }
        if !self.entity.on_ground {
            speed /= 5.0;
        }
        // A block is "harvestable" (normal speed, /30) when it needs no specific
        // tool OR we hold the right one. Only tool-required blocks (stone/ores/
        // metal) are 5× slower by hand (/100). Wood/dirt/leaves/sand need no tool.
        let needs_tool = is_pickaxe_block(&name)
            || name.ends_with("_block") && (name.contains("iron") || name.contains("gold") || name.contains("diamond") || name.contains("copper") || name.contains("netherite"))
            || name.contains("anvil") || name.contains("furnace");
        let can_harvest = !needs_tool || tool > 1.0; // the TOOL decides harvestability, not the penalized speed
        let damage = speed / hardness / if can_harvest { 30.0 } else { 100.0 };
        if damage >= 1.0 {
            return Duration::ZERO;
        }
        let ticks = (1.0 / damage).ceil() as u64;
        Duration::from_millis(ticks * 50)
    }

    /// Place the held item against a block face.
    pub async fn place_block(&mut self, x: i32, y: i32, z: i32, face: Face) -> std::io::Result<()> {
        site("place_block");
        self.sequence += 1;
        let seq = self.sequence;
        let held = self.held_item().map(|i| i.name.clone());
        if std::env::var("CAST_SNIFF").is_ok() {
            eprintln!("    SNIFF >use_item_on ({x},{y},{z}) face={face:?} held={held:?}");
        }
        self.client
            .write(
                "use_item_on",
                PValue::compound(vec![
                    ("hand", PValue::num(0.0)),
                    ("location", block_pos(x, y, z)),
                    ("direction", PValue::num(face as i32 as f64)),
                    ("cursorX", PValue::num(0.5)),
                    ("cursorY", PValue::num(0.5)),
                    ("cursorZ", PValue::num(0.5)),
                    ("insideBlock", PValue::Bool(false)),
                    ("worldBorderHit", PValue::Bool(false)),
                    ("sequence", PValue::num(seq as f64)),
                ]),
            )
            .await?;
        // Predict the placement: the server places the block but (1.19+) sends no
        // block_update back to the placer, so reflect it locally — the new block
        // appears one step out from the clicked face.
        let (ox, oy, oz) = match face {
            Face::Bottom => (0, -1, 0),
            Face::Top => (0, 1, 0),
            Face::North => (0, 0, -1),
            Face::South => (0, 0, 1),
            Face::West => (-1, 0, 0),
            Face::East => (1, 0, 0),
        };
        let (px, py, pz) = (x + ox, y + oy, z + oz);
        // Never predict a block INTO our own body: the server rejects that placement, but a local
        // prediction would leave a GHOST block our physics collides with (the bot then can't walk
        // west along a platform row it just "built" — 3 gym bots stuck a0 on every cell).
        let p = self.entity.position;
        let inside = (px as f64) < p.x + 0.3
            && (px as f64 + 1.0) > p.x - 0.3
            && (pz as f64) < p.z + 0.3
            && (pz as f64 + 1.0) > p.z - 0.3
            && (py as f64) < p.y + 1.8
            && (py as f64 + 1.0) > p.y;
        if !inside && self.block_state_at(px, py, pz) == 0 {
            let state = held.and_then(|n| {
                let key = n.strip_prefix("minecraft:").unwrap_or(&n).to_string();
                self.registry.blocks_by_name.get(&key).map(|b| b.default_state)
            });
            if let Some(state) = state {
                self.world.set_block_state_id(vec3(px as f64, py as f64, pz as f64), state);
            }
        }
        Ok(())
    }

    /// Walk to within ~2 blocks of (x,y,z). Returns `true` if the goal is met.
    pub async fn goto(&mut self, x: i32, y: i32, z: i32) -> std::io::Result<bool> {
        let goal = GoalNear::new(x as f64, y as f64, z as f64, 2.0);
        self.goto_goal(&goal, Duration::from_secs(18)).await
    }

    /// Walk to within `range` blocks of (x,y,z).
    pub async fn goto_near(&mut self, x: i32, y: i32, z: i32, range: f64) -> std::io::Result<bool> {
        let goal = GoalNear::new(x as f64, y as f64, z as f64, range);
        self.goto_goal(&goal, Duration::from_secs(18)).await
    }

    /// Like `goto_near` but with a custom time budget — use a short one for fail-fast
    /// reaches (e.g. approaching a possibly-unreachable embedded ore) so the caller
    /// doesn't burn the default 18s per attempt cycling many candidates.
    pub async fn goto_near_timeout(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        range: f64,
        timeout: Duration,
    ) -> std::io::Result<bool> {
        let goal = GoalNear::new(x as f64, y as f64, z as f64, range);
        self.goto_goal(&goal, timeout).await
    }

    /// Walk to within `range` blocks horizontally of (x,z) — at any reachable Y.
    /// Use for descending to something (e.g. a tree column in a valley).
    pub async fn goto_xz(&mut self, x: i32, z: i32, range: f64) -> std::io::Result<bool> {
        let goal = GoalNearXZ::new(x as f64, z as f64, range);
        self.goto_goal(&goal, Duration::from_secs(18)).await
    }

    fn goal_reached(&self, goal: &dyn Goal) -> bool {
        let p = self.entity.position;
        goal.is_end(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
    }

    /// Navigate to a goal: compute an A* path, follow it (digging obstacles,
    /// jumping, dropping), and re-path when stuck or the path runs out before the
    /// goal. Single-task port of typecraft's tick-driven pathfinder follower.
    /// A* in 40 ms slices with one driven tick between slices, same 2 s total budget as before.
    /// A single synchronous search froze the tick loop, and with it the breath watchdog, for up to
    /// 2 s per call. Race i6 logged `LOOP STALL 4060 ms` right after each `moving toward …` goto,
    /// and its breath alarm fired at 11.3 s against the 6 s rule. The search state lives in AStar;
    /// Movements only borrows the world, so it is rebuilt per slice.
    async fn plan_path(&mut self, start: (i32, i32, i32), goal: &dyn Goal, total: Duration) -> std::io::Result<PathResult> {
        // ASTAR_SYNC=1: the old one-shot search (6b-head's), to isolate the slicing in the tree-vs-head
        // portal gap (cycle 5: tree + search-time budget 11/23 vs 6b-head 16/26). No ticks during the search.
        static SYNC: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *SYNC.get_or_init(|| std::env::var("ASTAR_SYNC").ok().as_deref() == Some("1")) {
            return Ok(crate::path::get_path_to(&self.world, start, goal, self.movement.clone(), -1.0, total));
        }
        // Ticks now run while planning, so release the movement keys first: the old synchronous
        // search froze the bot in place, and walking on stale keys for up to 2 s near lava is not
        // safe. Sneak (edge safety) and the watchdog's jump are left as they are.
        for k in ["forward", "back", "left", "right", "sprint"] {
            self.set_control_state(k, false);
        }
        let mut astar = AStar::new(Move::start(start.0, start.1, start.2), goal, -1.0);
        loop {
            let r = {
                let mv = Movements::new(&self.world, self.movement.clone());
                astar.compute(goal, &mv, Duration::from_millis(40), total)
            };
            if r.status != PathStatus::Partial {
                return Ok(r);
            }
            self.drive_tick().await?;
        }
    }

    pub async fn goto_goal(&mut self, goal: &dyn Goal, timeout: Duration) -> std::io::Result<bool> {
        site("goto_goal");
        // Breath alarm: refuse to path at all, so a step looping over targets (gather_wood tried
        // 20+ logs, ~5 s each, for 120 s inside a capped flooded cave) fails fast and returns to
        // the main loop, whose survival escape is what can actually get out.
        if self.breath_alarm {
            self.clear_control_states();
            return Ok(false);
        }
        let started = Instant::now();
        let mut retries = 0;
        loop {
            if self.goal_reached(goal) {
                self.clear_control_states();
                self.wait_ticks(2).await?;
                return Ok(true);
            }
            if started.elapsed() > timeout {
                self.clear_control_states();
                return Ok(self.goal_reached(goal));
            }
            let start = (
                self.entity.position.x.floor() as i32,
                self.entity.position.y.floor() as i32,
                self.entity.position.z.floor() as i32,
            );
            let t_path = Instant::now();
            let result = self.plan_path(start, goal, Duration::from_millis(2000)).await?;
            // Stall hunt: two 14.05 s LOOP STALLs right after a water descent step.
            if t_path.elapsed() > Duration::from_millis(2500) {
                eprintln!("[bot] SLOW A* {} ms from {start:?} status={:?} len={}", t_path.elapsed().as_millis(), result.status, result.path.len());
            }
            if std::env::var("GOTO_DEBUG").is_ok() && retries == 0 {
                let end = result.path.last().map(|m| (m.x, m.y, m.z));
                eprintln!("GOTO from {start:?} status={:?} len={} end={end:?}", result.status, result.path.len());
            }
            if result.path.is_empty() {
                if result.status == PathStatus::NoPath {
                    self.clear_control_states();
                    return Ok(false);
                }
                retries += 1;
                if retries > 6 {
                    self.clear_control_states();
                    return Ok(false);
                }
                self.wait_ticks(4).await?;
                continue;
            }
            let before_follow = self.entity.position;
            let outcome = self.follow_path(&result.path).await?;
            // ALWAYS drive at least one tick per iteration. When follow_path returns without
            // moving (every waypoint already "reached"), this loop re-ran A* back to back with no
            // packet pumping — each search up to its 2 s budget — and the server saw a frozen
            // client: LOOP STALLs of 14.05 / 16.05 / 18.11 s (= 7 / 8 / 9 × 2 s, site follow_path)
            // and two `disconnect.timeout` kicks.
            self.wait_ticks(1).await?;
            match outcome {
                FollowOutcome::Reached => {
                    // Reached the path's end without moving and the goal still isn't met → the
                    // path is degenerate; another identical A* won't help.
                    let moved = (self.entity.position.x - before_follow.x).abs() + (self.entity.position.z - before_follow.z).abs();
                    if moved < 0.05 && !self.goal_reached(goal) {
                        self.clear_control_states();
                        return Ok(false);
                    }
                    // Reached the path's end; loop re-checks the goal / re-paths.
                    retries += 1;
                    if retries > 12 {
                        self.clear_control_states();
                        return Ok(self.goal_reached(goal));
                    }
                }
                FollowOutcome::NeedRepath => {
                    retries += 1;
                    if retries > 12 {
                        self.clear_control_states();
                        return Ok(self.goal_reached(goal));
                    }
                }
                FollowOutcome::Hurt => {
                    self.clear_control_states();
                    return Ok(false);
                }
                FollowOutcome::Disconnected => return Ok(false),
            }
        }
    }

    /// Follow a fixed path until it ends, the bot gets stuck, or it needs to
    /// place a block (unsupported → re-path). Digs `to_break` blocks in the way.
    async fn follow_path(&mut self, path: &[Move]) -> std::io::Result<FollowOutcome> {
        site("follow_path");
        // Start at the waypoint nearest the bot (skip already-passed nodes).
        let p = self.entity.position;
        let mut idx = 0;
        let mut best = f64::MAX;
        for (i, m) in path.iter().enumerate() {
            let dx = m.x as f64 + 0.5 - p.x;
            let dz = m.z as f64 + 0.5 - p.z;
            let d = dx * dx + dz * dz;
            if d < best {
                best = d;
                idx = i;
            }
        }

        let follow_start = Instant::now();
        let mut last_progress = Instant::now();
        let mut dig_progress = 0usize;
        let mut last_xz = (self.entity.position.x, self.entity.position.z);
        let mut stuck_ticks = 0u32;
        let debug = std::env::var("FOLLOW_DEBUG").is_ok();
        let mut dbgi = 0u32;
        let hp0 = self.health;
        while idx < path.len() {
            // Walking into lava/fire the local world didn't show (rust-gym-001 natural: died inside
            // `approach source → goto_near`, hp 20 → 0 with no chance to bail): stop the instant
            // health drops and hand control back — the callers all have retreat + heal logic.
            if self.health <= 0.0 || self.health < hp0 - 2.5 || self.breath_alarm {
                // breath_alarm: head under water > 6 s despite the watchdog's jump — return so the
                // main loop's survival escape (leave_water, with cap-dig) takes over.
                self.clear_control_states();
                return Ok(FollowOutcome::Hurt);
            }
            if debug && dbgi % 8 == 0 {
                let pp = self.entity.position;
                let n = &path[idx];
                eprintln!(
                    "FOLLOW idx={idx}/{} bot=({:.1},{:.1},{:.1}) wp=({},{},{}) dy={:.1} stuck={stuck_ticks} ground={} break={}",
                    path.len(), pp.x, pp.y, pp.z, n.x, n.y, n.z, n.y as f64 - pp.y, self.entity.on_ground, n.to_break.len()
                );
            }
            dbgi += 1;
            // Hard cap so one path segment can't exceed the overall goto budget.
            if follow_start.elapsed() > Duration::from_secs(12) {
                self.clear_control_states();
                return Ok(FollowOutcome::NeedRepath);
            }
            let next = &path[idx];
            let p = self.entity.position;
            let dx = next.x as f64 + 0.5 - p.x;
            let dz = next.z as f64 + 0.5 - p.z;
            let dy = next.y as f64 - p.y;
            // LIVE lava check against the current world, not the one A* planned on (lava flows; a
            // slice-planned path can be seconds old): the waypoint's feet or floor cell is lava now →
            // re-plan. And never sprint toward a waypoint with lava within 1 — sprint momentum carried
            // bots off lake edges (6c+lsm8 rust-gym-004: walking west at −54, feet cell lava at −55).
            let lava_at = |b: &Self, x: i32, y: i32, z: i32| {
                b.registry.blocks_by_state_id.get(&b.block_state_at(x, y, z)).map(|bl| bl.name.contains("lava")).unwrap_or(false)
            };
            let safe = crate::learn::safe_fixes();
            if safe && (lava_at(self, next.x, next.y, next.z) || lava_at(self, next.x, next.y - 1, next.z)) {
                self.clear_control_states();
                return Ok(FollowOutcome::NeedRepath);
            }
            let lava_close = safe && (-1..=1).any(|ox| (-1..=1).any(|oz| (-1..=0).any(|oy| lava_at(self, next.x + ox, next.y + oy, next.z + oz))));

            // Reached the waypoint only when at/above its level (dy <= 0.6) —
            // for an upward step this forces the bot to actually CLIMB before
            // advancing (a loose dy tolerance let it skip climbs and stall at the
            // base of ledges); for a drop it's already above, so it advances.
            if dx * dx + dz * dz <= 0.49 && dy <= 0.6 {
                idx += 1;
                dig_progress = 0;
                last_progress = Instant::now();
                continue;
            }

            // Dig any blocks the move requires breaking, one at a time.
            if dig_progress < next.to_break.len() {
                let (bx, by, bz) = next.to_break[dig_progress];
                if self.block_state_at(bx, by, bz) != 0 {
                    self.clear_control_states();
                    self.dig(bx, by, bz).await?;
                }
                dig_progress += 1;
                last_progress = Instant::now();
                continue;
            }

            // Block placement (scaffolding) isn't supported yet — re-path around it.
            if !next.to_place.is_empty() {
                self.clear_control_states();
                return Ok(FollowOutcome::NeedRepath);
            }

            // Walk toward the waypoint; jump to step up, for parkour, or to clear
            // a lip when we've stopped making horizontal progress (anti-wedge).
            let mx = next.x as f64 + 0.5 - p.x;
            let mz = next.z as f64 + 0.5 - p.z;
            self.look((-mx).atan2(-mz), 0.0);
            self.set_control_state("forward", true);
            self.set_control_state("sprint", !lava_close);
            // Jump to climb only when CLOSE to the up-step (so we walk up to it with
            // ground momentum and step onto it), or for parkour. Jumping while far
            // from the step just bounces in open air with no forward progress.
            let near = dx * dx + dz * dz < 1.6;
            self.set_control_state("jump", next.parkour || (next.y as f64 > p.y + 0.5 && near));

            let step = self.drive_tick().await?;
            if matches!(step, DriveStep::Disconnected) {
                return Ok(FollowOutcome::Disconnected);
            }

            // Only judge "stuck" on actual PHYSICS ticks — the loop also spins on
            // packet I/O (no movement), and counting those as stuck makes the bot
            // re-path constantly and crawl. One physics tick = one chance to move.
            if matches!(step, DriveStep::Tick) {
                let np = self.entity.position;
                let moved = (np.x - last_xz.0).powi(2) + (np.z - last_xz.1).powi(2);
                if moved > 0.0009 {
                    last_xz = (np.x, np.z);
                    stuck_ticks = 0;
                } else {
                    stuck_ticks += 1;
                }
            }

            // Re-path when genuinely stuck: ~40 ticks with no horizontal movement
            // (tick-based — reliable even when the loop spins on packet I/O), or
            // 2.5 s wall-clock without reaching the next waypoint.
            if stuck_ticks >= 40 || last_progress.elapsed() > Duration::from_millis(2500) {
                self.clear_control_states();
                return Ok(FollowOutcome::NeedRepath);
            }
        }
        self.clear_control_states();
        Ok(FollowOutcome::Reached)
    }

    /// Stop using the held item (player_action status 5): releases a drawn bow, firing the arrow along
    /// the current look. Pair with `activate_item` to start the draw (≥ 20 ticks = full power).
    pub async fn release_use_item(&mut self) -> std::io::Result<()> {
        self.sequence += 1;
        let seq = self.sequence;
        // The look itself reaches the server with each tick's movement packet during the draw.
        self.client
            .write(
                "player_action",
                PValue::compound(vec![
                    ("status", PValue::num(5.0)),
                    ("location", block_pos(0, 0, 0)),
                    ("face", PValue::num(0.0)),
                    ("sequence", PValue::num(seq as f64)),
                ]),
            )
            .await
    }

    pub async fn respawn(&mut self) -> std::io::Result<()> {
        self.client
            .write("client_command", PValue::compound(vec![("actionId", PValue::num(0.0)), ("payload", PValue::num(0.0))]))
            .await
    }
}

fn loc_xyz(loc: &PValue) -> (i32, i32, i32) {
    (
        loc.get("x").and_then(PValue::as_i32).unwrap_or(0),
        loc.get("y").and_then(PValue::as_i32).unwrap_or(0),
        loc.get("z").and_then(PValue::as_i32).unwrap_or(0),
    )
}

/// Vanilla block hardness (destroy time) by name. The generated registry has no real values (see
/// `dig_time`), and a break time computed from the wrong hardness makes the server reject the dig.
/// Exact names first, then families; `None` = unknown (caller falls back to the registry).
pub fn block_hardness(name: &str) -> Option<f64> {
    let exact = match name {
        "bedrock" | "end_portal_frame" | "end_portal" | "nether_portal" | "barrier" | "command_block" => return Some(-1.0),
        "obsidian" | "crying_obsidian" | "respawn_anchor" => 50.0,
        "ancient_debris" => 30.0,
        "netherite_block" => 50.0,
        "ender_chest" => 22.5,
        "spawner" | "iron_block" | "diamond_block" | "emerald_block" | "enchanting_table" => 5.0,
        "cobbled_deepslate" | "polished_deepslate" | "deepslate_bricks" | "deepslate_tiles" | "chiseled_deepslate" | "furnace" | "blast_furnace" | "smoker" | "dispenser" | "dropper" | "iron_bars" | "lodestone" => 3.5,
        "deepslate" | "end_stone" | "end_stone_bricks" | "gold_block" | "copper_block" | "nether_quartz_ore" | "nether_gold_ore" | "beacon" | "anvil" | "chipped_anvil" | "damaged_anvil" | "iron_trapdoor" | "brewing_stand" => 3.0,
        "crafting_table" | "chest" | "trapped_chest" | "barrel" | "smithing_table" | "fletching_table" | "cartography_table" | "loom" | "jukebox" | "bookshelf" | "chiseled_bookshelf" | "lectern" => 2.5,
        "cobblestone" | "mossy_cobblestone" | "cobblestone_stairs" | "cobblestone_slab" | "cobblestone_wall" | "bricks" | "nether_bricks" | "red_nether_bricks" | "nether_brick_fence" | "nether_brick_stairs" | "nether_brick_slab" | "nether_brick_wall" | "bone_block" | "basalt_slab" | "stone_slab" | "smooth_stone" | "smooth_stone_slab" | "stone_stairs" | "sandstone_stairs" | "mud_brick_wall" => 2.0,
        "stone" | "granite" | "diorite" | "andesite" | "polished_granite" | "polished_diorite" | "polished_andesite" | "tuff" | "stone_bricks" | "mossy_stone_bricks" | "cracked_stone_bricks" | "chiseled_stone_bricks" | "blackstone" | "polished_blackstone" | "polished_blackstone_bricks" | "gilded_blackstone" | "dripstone_block" | "pointed_dripstone" | "amethyst_block" | "budding_amethyst" | "purpur_block" | "purpur_pillar" | "prismarine" | "dark_prismarine" | "prismarine_bricks" | "mud_bricks" | "infested_stone" | "smooth_sandstone" | "cut_sandstone" => 1.5,
        "terracotta" | "smooth_basalt" | "basalt" | "polished_basalt" | "nether_bricks_slab" => 1.25,
        "packed_mud" | "bamboo" | "bamboo_block" | "nether_wart_block" | "warped_wart_block" | "shroomlight" | "pumpkin" | "carved_pumpkin" | "jack_o_lantern" | "melon" | "dried_kelp_block" | "coral_block" => 1.0,
        "sandstone" | "red_sandstone" | "chiseled_sandstone" | "quartz_block" | "smooth_quartz" | "quartz_pillar" | "chiseled_quartz_block" | "quartz_bricks" | "netherrack_slab" => 0.8,
        "calcite" => 0.75,
        "grass_block" | "gravel" | "clay" | "mycelium" | "podzol" | "dirt_path" | "rooted_dirt" | "sponge" | "wet_sponge" | "honeycomb_block" => 0.6,
        "dirt" | "coarse_dirt" | "sand" | "red_sand" | "soul_sand" | "soul_soil" | "mud" | "muddy_mangrove_roots" | "magma_block" | "ice" | "packed_ice" | "blue_ice" | "hay_block" | "target" | "farmland" | "snow_block" | "powder_snow" | "sculk_catalyst" => 0.5,
        "netherrack" | "cactus" | "chorus_plant" | "chorus_flower" | "ladder" => 0.4,
        "glowstone" | "sea_lantern" | "glass" | "tinted_glass" | "glass_pane" | "redstone_lamp" => 0.3,
        "snow" | "sculk" | "sculk_vein" | "sculk_sensor" | "sculk_shrieker" | "brown_mushroom_block" | "red_mushroom_block" | "mushroom_stem" | "cake" | "vine" | "glow_lichen" | "hanging_roots" => 0.2,
        "moss_block" | "moss_carpet" | "azalea" | "flowering_azalea" | "big_dripleaf" | "small_dripleaf" | "nether_sprouts" | "sea_pickle" | "carpet" | "white_carpet" | "moss" => 0.1,
        "torch" | "wall_torch" | "soul_torch" | "soul_wall_torch" | "redstone_torch" | "tnt" | "slime_block" | "honey_block" | "fire" | "soul_fire" | "scaffolding" | "redstone_wire" | "tripwire" | "tripwire_hook" | "lever" | "flower_pot" | "sugar_cane" | "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass" | "lily_pad" | "crimson_fungus" | "warped_fungus" | "crimson_roots" | "warped_roots" | "twisting_vines" | "weeping_vines" | "twisting_vines_plant" | "weeping_vines_plant" | "cave_vines" | "cave_vines_plant" | "spore_blossom" | "cobweb_plant" => 0.0,
        _ => f64::NAN,
    };
    if !exact.is_nan() {
        return Some(exact);
    }
    // Families.
    if name.ends_with("_ore") {
        return Some(if name.starts_with("deepslate_") { 4.5 } else { 3.0 });
    }
    if name.ends_with("_log") || name.ends_with("_wood") || name.ends_with("_stem") || name.ends_with("_hyphae") || name.ends_with("_planks") || name.ends_with("_fence") || name.ends_with("_fence_gate") || name.ends_with("_stairs") || name.ends_with("_slab") || name.ends_with("_door") || name.ends_with("_trapdoor") || name.ends_with("_wall") || name == "mangrove_roots" {
        return Some(2.0);
    }
    if name.ends_with("_leaves") || name.ends_with("_sapling") || name.ends_with("_propagule") {
        return Some(0.2);
    }
    if name.ends_with("_wool") || name.ends_with("_glazed_terracotta") || name.ends_with("_bed") {
        return Some(if name.ends_with("_bed") { 0.2 } else if name.ends_with("_wool") { 0.8 } else { 1.4 });
    }
    if name.ends_with("_terracotta") {
        return Some(1.25);
    }
    if name.ends_with("_concrete") {
        return Some(1.8);
    }
    if name.ends_with("_concrete_powder") {
        return Some(0.5);
    }
    if name.ends_with("_coral") || name.ends_with("_coral_fan") || name.ends_with("_coral_wall_fan") || name.ends_with("_carpet") || name.ends_with("_candle") || name.contains("mushroom") && !name.ends_with("_block") {
        return Some(0.0);
    }
    if name.ends_with("_coral_block") {
        return Some(1.5);
    }
    if name.ends_with("_stained_glass") || name.ends_with("_stained_glass_pane") {
        return Some(0.3);
    }
    if name.ends_with("_shulker_box") || name == "shulker_box" {
        return Some(2.0);
    }
    if name.contains("copper") {
        return Some(3.0);
    }
    if name.contains("amethyst") {
        return Some(1.5);
    }
    if name.contains("sandstone") {
        return Some(0.8);
    }
    if name.contains("basalt") {
        return Some(1.25);
    }
    if name.contains("blackstone") || name.contains("stone_brick") || name.contains("prismarine") || name.contains("purpur") || name.contains("deepslate_") && !name.ends_with("_ore") {
        return Some(if name.contains("deepslate_") { 3.5 } else { 1.5 });
    }
    if name.contains("nether_brick") || name.contains("cobblestone") || name.contains("brick") {
        return Some(2.0);
    }
    if name.contains("stone") {
        return Some(1.5);
    }
    if name.ends_with("_button") || name.ends_with("_pressure_plate") || name.ends_with("_sign") || name.ends_with("_hanging_sign") || name.ends_with("_banner") {
        return Some(if name.ends_with("_sign") || name.ends_with("_hanging_sign") || name.ends_with("_banner") { 1.0 } else { 0.5 });
    }
    if name.contains("flower") || name.contains("grass") || name.contains("fern") || name.contains("bush") || name.contains("dandelion") || name.contains("poppy") || name.contains("tulip") || name.contains("orchid") || name.contains("allium") || name.contains("daisy") || name.contains("lilac") || name.contains("peony") || name.contains("rose") || name.contains("lily") || name.contains("wheat") || name.contains("carrots") || name.contains("potatoes") || name.contains("beetroots") || name.contains("nether_wart") || name.contains("sweet_berry") || name.contains("dead_bush") || name.contains("_root") || name.contains("seed") || name.contains("pitcher") || name.contains("torchflower") {
        return Some(0.0);
    }
    None
}

/// Blocks mined with a pickaxe (and dropping nothing by hand). The old test was a substring check
/// for stone/ore/brick/deepslate, which missed granite, diorite, andesite, tuff, calcite, basalt,
/// blackstone, netherrack, obsidian… — those got the BARE-HAND break time, ~3× too long, so every
/// dig held that long: 4 s per descent block in diorite vs 1.5 s in stone (rust-gym-003, timed).
pub fn is_pickaxe_block(name: &str) -> bool {
    const FAMILY: [&str; 24] = [
        "stone", "_ore", "brick", "deepslate", "granite", "diorite", "andesite", "tuff", "calcite", "basalt",
        "blackstone", "netherrack", "obsidian", "terracotta", "concrete", "prismarine", "purpur", "dripstone",
        "sandstone", "quartz", "magma_block", "end_stone", "amethyst", "copper",
    ];
    FAMILY.iter().any(|f| name.contains(f)) && !name.contains("powder") && !name.ends_with("_button") && !name.contains("pressure_plate")
}

/// Coarse tool-speed multiplier (typecraft's fallback when material data is absent).
fn tool_speed(tool: Option<&str>, block: Option<String>) -> f64 {
    let (Some(tool), Some(block)) = (tool, block) else {
        return 1.0;
    };
    let tier = match tool.split('_').next().unwrap_or("") {
        "wooden" => 2.0,
        "stone" => 4.0,
        "iron" => 6.0,
        "golden" => 12.0,
        "diamond" => 8.0,
        "netherite" => 9.0,
        _ => return 1.0,
    };
    let ttype = tool.rsplit('_').next().unwrap_or("");
    let pick = is_pickaxe_block(&block);
    let axe = block.contains("log") || block.contains("planks") || block.contains("wood");
    let shovel = block.contains("dirt") || block.contains("sand") || block.contains("gravel") || block == "grass_block";
    match ttype {
        "pickaxe" if pick => tier,
        "axe" if axe => tier,
        "shovel" if shovel => tier,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_speed_matches() {
        assert_eq!(tool_speed(Some("stone_pickaxe"), Some("stone".into())), 4.0);
        assert_eq!(tool_speed(Some("wooden_axe"), Some("oak_log".into())), 2.0);
        assert_eq!(tool_speed(Some("diamond_pickaxe"), Some("dirt".into())), 1.0);
        assert_eq!(tool_speed(None, Some("stone".into())), 1.0);
    }
}
