//! The tunnel's memory loads from a moment written before its newer fields (agent R's review).

#[test]
fn a_travel_with_fields_missing_still_loads() {
    let t: plan::tunnel::Travel = toml::from_str("routes = 3").unwrap();
    assert_eq!(t.routes, 3);
    assert!(t.route.is_empty() && t.blocked.is_empty() && t.target.is_none());
}
