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

#[test]
fn sub_errors_on_connection_failure() {
    let mut cmd = Command::cargo_bin("moqtail-cli").unwrap();
    cmd.arg("sub").arg("/foo").arg("--host").arg("invalid");
    cmd.assert().failure().stderr(contains("Connection error"));
}

#[test]
fn sub_accepts_auth_and_tls_flags() {
    let mut cmd = Command::cargo_bin("moqtail-cli").unwrap();
    cmd.arg("sub")
        .arg("/foo")
        .arg("--username")
        .arg("user")
        .arg("--password")
        .arg("pass")
        .arg("--tls")
        .arg("--dry-run");
    cmd.assert().success().stdout(contains("/foo"));
}

#[test]
fn sub_accepts_single_credential_flags() {
    let mut cmd = Command::cargo_bin("moqtail-cli").unwrap();
    cmd.arg("sub")
        .arg("/foo")
        .arg("--username")
        .arg("user")
        .arg("--dry-run");
    cmd.assert().success().stdout(contains("/foo"));

    let mut cmd = Command::cargo_bin("moqtail-cli").unwrap();
    cmd.arg("sub")
        .arg("/foo")
        .arg("--password")
        .arg("pass")
        .arg("--dry-run");
    cmd.assert().success().stdout(contains("/foo"));
}
