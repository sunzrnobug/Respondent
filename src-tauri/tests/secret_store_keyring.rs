use std::time::{SystemTime, UNIX_EPOCH};

use respondent_lib::secret_store::roundtrip_system_keyring_for_integration_test;

fn unique_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    format!("integration-test-{nanos}/llm-api-key")
}

#[test]
#[ignore = "writes to the OS credential store (Windows Credential Manager / Keychain / Secret Service)"]
fn system_keyring_roundtrips_provider_secret() {
    std::env::remove_var("RESPONDENT_SECRET_BACKEND");
    let key = unique_key();
    let value = format!("sk-integration-{}", unique_key());

    roundtrip_system_keyring_for_integration_test(&key, &value)
        .unwrap_or_else(|err| panic!("system keyring roundtrip failed: {err}"));
}
