use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

use super::*;

fn output(code: i32, stdout: &str, stderr: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(code),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn check_step_ok_sur_exit_zero() {
    let out = output(0, "tout va bien", "");
    assert!(check_step(&out, "étape").is_ok());
}

#[test]
fn check_step_err_sur_exit_non_zero() {
    let out = output(1, "", "pg_dump: erreur de connexion");
    let err = check_step(&out, "dump SQL (pg_dump)").unwrap_err();
    assert!(err.to_string().contains("dump SQL (pg_dump)"));
    assert!(err.to_string().contains("pg_dump: erreur de connexion"));
}
