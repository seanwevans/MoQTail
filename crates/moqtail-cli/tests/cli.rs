use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn subprints_compiled_selector() {
    let mut cmd = Command::cargo_bin("moqtail-cli").unwrap();
    cmd.arg("sub").arg("/foo").arg("--dry-run");
    cmd.assert().success().stdout(contains("/foo"));
}

#[test]
fn sub_errors_on_invalid_selector() {
    let mut cmd = Command::cargo_bin("moqtail-cli").unwrap();
    cmd.arg("sub").arg("foo").arg("--dry-run");
    cmd.assert()
        .failure()
        .stderr(contains("Failed to compile selector"));
}
