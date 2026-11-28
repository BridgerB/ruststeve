//! The sprint's constants are config/sprint.toml's: the core reads no files, so it mirrors the
//! numbers S1 read from the 26.1.2 jars, and this test holds the two equal.

use physics::{
    AIR_ACCELERATION, FORWARD_INPUT, GROUND_INERTIA, MINOR_COLLISION_ANGLE, MINOR_COLLISION_MIN_SQ,
    SPRINT_AIR_ACCELERATION, SPRINT_FOOD_FLOOR, SPRINT_JUMP_BOOST, SPRINT_SPEED_BOOST,
    ground_acceleration,
};

const CONFIG: &str = include_str!("../../../config/sprint.toml");

fn number(table: &toml::Table, section: &str, key: &str) -> Result<f64, String> {
    let value = table
        .get(section)
        .and_then(|s| s.get(key))
        .ok_or_else(|| format!("config/sprint.toml has no [{section}] {key}"))?;
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
        .ok_or_else(|| format!("[{section}] {key} is not a number"))
}

/// Equal to the float the jar holds: the walk's 0.1 and the sprint's 0.3 are round values where
/// the jar stores the float (0.30000001192092896), so equal within a float's rounding.
fn near(ours: f64, theirs: f64, what: &str) -> Result<(), String> {
    if (ours - theirs).abs() <= 1e-7 * theirs.abs().max(1.0) {
        Ok(())
    } else {
        Err(format!(
            "{what}: physics {ours}, config/sprint.toml {theirs}"
        ))
    }
}

#[test]
fn the_constants_are_config_sprint_toml() -> Result<(), String> {
    let t: toml::Table = CONFIG
        .parse()
        .map_err(|e| format!("config/sprint.toml: {e}"))?;
    near(
        SPRINT_SPEED_BOOST,
        number(&t, "speed", "sprint_modifier_amount")?,
        "sprint modifier",
    )?;
    near(
        AIR_ACCELERATION,
        number(&t, "speed", "air_acceleration_walk")?,
        "air acceleration",
    )?;
    near(
        SPRINT_AIR_ACCELERATION,
        number(&t, "speed", "air_acceleration_sprint")?,
        "sprint air acceleration",
    )?;
    near(
        SPRINT_JUMP_BOOST,
        number(&t, "jump", "sprint_jump_boost")?,
        "sprint jump boost",
    )?;
    near(
        f64::from(SPRINT_FOOD_FLOOR),
        number(&t, "start", "food_above")?,
        "food floor",
    )?;
    near(
        MINOR_COLLISION_ANGLE,
        number(&t, "stop", "minor_collision_radians")?,
        "minor collision angle",
    )?;
    near(
        MINOR_COLLISION_MIN_SQ,
        number(&t, "stop", "minor_collision_min_sq")?,
        "minor collision minimum",
    )?;
    // The derived steady paces, to the config's six places.
    let steady = |accel: f64| accel * FORWARD_INPUT / (1.0 - GROUND_INERTIA);
    let walk = steady(ground_acceleration());
    let sprint = steady(ground_acceleration() * (1.0 + SPRINT_SPEED_BOOST));
    let walk_cfg = number(&t, "speed", "steady_walk_per_tick")?;
    let sprint_cfg = number(&t, "speed", "steady_sprint_per_tick")?;
    if (walk - walk_cfg).abs() > 1e-6 || (sprint - sprint_cfg).abs() > 1e-6 {
        return Err(format!(
            "steady paces: physics walk {walk} sprint {sprint}, config {walk_cfg} {sprint_cfg}"
        ));
    }
    Ok(())
}
