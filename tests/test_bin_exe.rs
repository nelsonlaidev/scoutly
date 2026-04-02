use std::process::Command;
#[test]
fn test_bin_exe() {
    let bin = env!("CARGO_BIN_EXE_scoutly");
    let output = Command::new(bin).args(["--help"]).output().unwrap();
    assert!(output.status.success());
}
