use serde_json::json;

use super::support::{harness, ok};

#[test]
fn metadata_when_invoked_through_the_frontend_contract() {
    let harness = harness("http://127.0.0.1:9");
    let info = headway_core::Headway::build_info();
    let result = ok(&harness, "get_build_info", json!({}));
    assert_eq!(result, serde_json::to_value(info).unwrap());
}

#[test]
fn app_version_remains_a_string() {
    let harness = harness("http://127.0.0.1:9");
    let result = ok(&harness, "app_version", json!({}));
    assert_eq!(
        result,
        json!(headway_core::Headway::build_info().to_string())
    );
}
