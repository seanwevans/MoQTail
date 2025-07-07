use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn subprints_compiled_selector() {
    let mut cmd = Command::cargo_bin("moqtail-cli").unwrap();
    cmd.arg("sub").arg("/foo");
    cmd.assert().success().stdout(contains("Selector"));
}
