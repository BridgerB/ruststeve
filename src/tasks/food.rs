//! Gather food — hunt passive animals with the sword, collect the meat. Port of
//! steve's `tasks/food`.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use rustcraft::bot::{Bot, DriveStep};
use rustcraft::vec3::vec3;

use crate::bot_utils::{collect_drops, select_item};
use crate::types::{failure, success, StepResult};

const ANIMALS: &[&str] = &["cow", "pig", "sheep", "chicken", "rabbit"];
const MEATS: &[&str] = &[
    "beef", "cooked_beef", "porkchop", "cooked_porkchop", "mutton", "cooked_mutton",
    "chicken", "cooked_chicken", "rabbit", "cooked_rabbit", "leather",
];

fn count_food(bot: &Bot) -> i32 {
    bot.inventory
        .slots
        .iter()
        .flatten()
        .filter(|i| MEATS.contains(&i.name.as_str()) && i.name != "leather")
        .map(|i| i.count)
        .sum()
}

fn animal_type_ids(bot: &Bot) -> HashSet<i32> {
    ANIMALS.iter().filter_map(|n| bot.registry.entities_by_name.get(*n).map(|d| d.id)).collect()
}

/// Nearest food animal entity id.
fn nearest_animal(bot: &Bot, ids: &HashSet<i32>) -> Option<i32> {
    let p = bot.entity.position;
    bot.entities
        .iter()
        .filter(|(_, e)| e.entity_type.map(|t| ids.contains(&t)).unwrap_or(false))
        .min_by(|(_, a), (_, b)| {
            let da = (a.position.x - p.x).powi(2) + (a.position.z - p.z).powi(2);
            let db = (b.position.x - p.x).powi(2) + (b.position.z - p.z).powi(2);
            da.partial_cmp(&db).unwrap()
        })
        .map(|(id, _)| *id)
}

pub async fn gather_food(bot: &mut Bot<'_>, target: i32) -> StepResult {
    for sword in ["diamond_sword", "iron_sword", "stone_sword", "wooden_sword"] {
        if select_item(bot, sword).await.unwrap_or(false) {
            break;
        }
    }
    let ids = animal_type_ids(bot);
    let deadline = Instant::now() + Duration::from_secs(150);
    let mut explore_dir = 0.0f64;

    while count_food(bot) < target && Instant::now() < deadline {
        let Some(id) = nearest_animal(bot, &ids) else {
            // No animal in sight — wander to load new entities.
            explore_dir += 1.3;
            for _ in 0..40 {
                bot.look(explore_dir, 0.0);
                bot.set_control_state("forward", true);
                bot.set_control_state("sprint", true);
                bot.set_control_state("jump", true);
                if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
                    bot.clear_control_states();
                    return failure("disconnected");
                }
            }
            bot.clear_control_states();
            continue;
        };

        // Chase and attack until it dies (leaves bot.entities) or we give up.
        let before_food = count_food(bot);
        let mut hits = 0;
        let (mut ax, mut az) = (0, 0);
        for _ in 0..80 {
            let Some(e) = bot.entities.get(&id) else { break };
            let ep = e.position;
            ax = ep.x.floor() as i32;
            az = ep.z.floor() as i32;
            let p = bot.entity.position;
            let dist = ((ep.x - p.x).powi(2) + (ep.z - p.z).powi(2)).sqrt();
            if dist > 2.2 {
                bot.look_at(vec3(ep.x, p.y + 1.0, ep.z));
                bot.set_control_state("forward", true);
                bot.set_control_state("sprint", true);
                if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
                    return failure("disconnected");
                }
            } else {
                bot.clear_control_states();
                bot.attack(id).await.ok();
                bot.wait_ticks(6).await.ok(); // attack cooldown
                hits += 1;
                if hits > 25 {
                    break;
                }
            }
        }
        bot.clear_control_states();
        // Pick up the drops where the animal fell.
        collect_drops(bot, ax, az).await;
        if count_food(bot) > before_food {
            println!("    food: {} food", count_food(bot));
        }
    }
    let n = count_food(bot);
    if n >= target {
        success(format!("gathered {n}/{target} food"))
    } else {
        failure(format!("gathered {n}/{target} food"))
    }
}
