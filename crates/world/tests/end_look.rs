//! Issue 20e (agent E3, docs/problems/end-island.md): a look asked for is sent though the ground
//! under the body is not loaded yet. The server judges an enderman's stare by the look it holds;
//! after the teleport into the End that is pitch 0 until a look packet comes (E5's race
//! 45913296602 lane 1: an enderman angered at tick 60 of the arrival).

use physics::Controls;
use protocol::{C2s, S2c};
use world::{Effect, Event, World};

#[test]
fn a_look_down_goes_out_before_the_ground_is_loaded() {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 7,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 2,
        dimension: "minecraft:the_end".to_owned(),
        game_type: 0,
    }));
    w.body = Some(physics::Body {
        pos: [100.5, 49.0, 0.5],
        ..physics::Body::default()
    });
    w.player.pitch = 0.0;
    w.step(&Event::Controls(Controls {
        yaw: 90.0,
        pitch: Some(90.0),
        ..Controls::default()
    }));
    let out = w.step(&Event::Tick {
        now_ms: 50,
        seed: 1,
    });
    assert!(
        out.iter().any(|e| matches!(
            e,
            Effect::Send(C2s::MovePlayerPosRot { pitch, .. }) if (*pitch - 90.0).abs() < 1e-3
        )),
        "{out:?}"
    );
    assert!((w.player.pitch - 90.0).abs() < 1e-3);
    // Sent once: the next tick with the same look sends no second one.
    let out = w.step(&Event::Tick {
        now_ms: 100,
        seed: 2,
    });
    assert!(
        !out.iter()
            .any(|e| matches!(e, Effect::Send(C2s::MovePlayerPosRot { .. }))),
        "{out:?}"
    );
}
