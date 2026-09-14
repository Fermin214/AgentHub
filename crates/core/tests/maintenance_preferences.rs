use agenthub_core::{
    maintenance::{dispatch, record_attempt},
    network,
    store::Store,
};
use serde_json::{json, Value};
fn get(store: &Store) -> Value {
    dispatch(store, "maintenance.get", &json!({})).unwrap()
}
#[test]
fn defaults_persist_independently_and_reject_invalid_preferences() {
    let t = tempfile::tempdir().unwrap();
    let store = Store::open(t.path()).unwrap();
    let app = store.get_settings().unwrap();
    assert_eq!(
        get(&store),
        json!({"automaticChecks":false,"intervalHours":24,"retainUpdateBackup":true,"maxBackups":null})
    );
    network::dispatch(
        &store,
        "network.save",
        &json!({"proxyUrl":"http://127.0.0.1:7897"}),
    )
    .unwrap();
    dispatch(
        &store,
        "maintenance.save",
        &json!({"automaticChecks":true,"intervalHours":168,"other":"ignore"}),
    )
    .unwrap();
    for bad in [
        json!({"automaticChecks":true,"intervalHours":0}),
        json!({"automaticChecks":true,"intervalHours":169}),
        json!({"automaticChecks":true,"intervalHours":1.5}),
        json!({"automaticChecks":"true","intervalHours":24}),
        json!({"retainUpdateBackup":"false"}),
    ] {
        assert!(dispatch(&store, "maintenance.save", &bad).is_err());
        assert_eq!(get(&store)["intervalHours"], 168);
    }
    drop(store);
    let store = Store::open(t.path()).unwrap();
    assert_eq!(
        get(&store),
        json!({"automaticChecks":true,"intervalHours":168,"retainUpdateBackup":true,"maxBackups":null})
    );
    assert_eq!(store.get_settings().unwrap(), app);
    assert_eq!(network::proxy_url(&store).unwrap(), "http://127.0.0.1:7897");
    dispatch(
        &store,
        "maintenance.save",
        &json!({"automaticChecks":false,"intervalHours":1}),
    )
    .unwrap();
    assert_eq!(get(&store)["automaticChecks"], false);
    dispatch(
        &store,
        "maintenance.save",
        &json!({"retainUpdateBackup":false}),
    )
    .unwrap();
    assert_eq!(get(&store)["retainUpdateBackup"], false);
    assert_eq!(get(&store)["intervalHours"], 1);
}
#[test]
fn the_attempt_timestamp_is_the_only_check_history_and_stays_store_local() {
    let t = tempfile::tempdir().unwrap();
    let store = Store::open(t.path()).unwrap();
    assert!(get(&store).get("lastAttemptAt").is_none());
    record_attempt(&store).unwrap();
    let recorded = get(&store);
    let stamp = recorded["lastAttemptAt"].as_str().unwrap().to_string();
    assert!(chrono::DateTime::parse_from_rfc3339(&stamp).is_ok());
    // Recording an attempt only throttles the next automatic check; it must not
    // enable checks or change any preference the user chose.
    assert_eq!(recorded["automaticChecks"], false);
    assert_eq!(recorded["intervalHours"], 24);
    drop(store);
    let reopened = Store::open(t.path()).unwrap();
    assert_eq!(get(&reopened)["lastAttemptAt"], json!(stamp));
    let isolated = Store::open_in_memory().unwrap();
    assert!(get(&isolated).get("lastAttemptAt").is_none());
}
