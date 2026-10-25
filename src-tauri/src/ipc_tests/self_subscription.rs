//! Self subscription: adopt the installed app once, without guessing a baseline.

use serde_json::json;

use super::support::{bundle_harness, call, ok};

#[test]
fn ensure_self_subscription_registers_the_installed_app_once() {
    let harness = bundle_harness();
    let view = ok(&harness, "ensure_self_subscription", json!({}));
    let id = view["app"]["id"].as_str().unwrap().to_owned();
    assert!(id.starts_with("headway"), "{view}");
    assert_eq!(view["app"]["auto_update"], false, "{view}");
    assert_eq!(view["app"]["source"], "github", "{view}");
    assert_eq!(view["app"]["track"]["mode"], "commit", "{view}");

    // Idempotent: asking again keeps the same single subscription.
    let again = ok(&harness, "ensure_self_subscription", json!({}));
    assert_eq!(again["app"]["id"], view["app"]["id"]);
    let apps = ok(&harness, "list_apps", json!({}));
    assert_eq!(apps.as_array().unwrap().len(), 1, "{apps}");
}

#[test]
fn ensure_self_subscription_without_a_bundle_is_an_input_error() {
    // Nothing installed: no bundle to adopt and no config write either.
    let harness = super::support::harness("http://127.0.0.1:9");
    let error = call(&harness, "ensure_self_subscription", json!({})).unwrap_err();
    assert_eq!(error["code"], "invalid_input");
    let apps = ok(&harness, "list_apps", json!({}));
    assert_eq!(
        apps,
        json!([]),
        "a failed adoption must not register anything"
    );
}
