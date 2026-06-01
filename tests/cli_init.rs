use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn init_writes_config_template() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("provider.yaml");

    Command::cargo_bin("octest")
        .unwrap()
        .arg("init")
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote config template"));

    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("base_url:"));
    assert!(content.contains("models:"));
}
