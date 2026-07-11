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

#[test]
fn stdout_str_trim_les_espaces() {
    let out = output(0, "  bonjour \n", "");
    assert_eq!(stdout_str(&out), "bonjour");
}

#[test]
fn stdout_str_vide_donne_chaine_vide() {
    let out = output(0, "", "");
    assert_eq!(stdout_str(&out), "");
}

#[test]
fn stdout_str_utf8_invalide_ne_panique_pas() {
    let out = Output {
        status: ExitStatus::from_raw(0),
        stdout: vec![0xff, 0xfe, b'x'],
        stderr: vec![],
    };
    // Conversion "lossy" : ne doit pas paniquer, le contenu exact importe peu ici.
    let _ = stdout_str(&out);
}

#[test]
fn stderr_str_trim_les_espaces() {
    let out = output(1, "", "  erreur \n");
    assert_eq!(stderr_str(&out), "erreur");
}

#[test]
fn stderr_str_vide_donne_chaine_vide() {
    let out = output(1, "", "");
    assert_eq!(stderr_str(&out), "");
}

#[test]
fn backups_to_purge_garde_conserve_moins_que_le_total() {
    let all = vec!["c.tar.gz".to_string(), "b.tar.gz".to_string(), "a.tar.gz".to_string()];
    assert_eq!(backups_to_purge(all, 1), vec!["b.tar.gz".to_string(), "a.tar.gz".to_string()]);
}

#[test]
fn backups_to_purge_keep_zero_purge_tout() {
    let all = vec!["b.tar.gz".to_string(), "a.tar.gz".to_string()];
    assert_eq!(backups_to_purge(all, 0), vec!["b.tar.gz".to_string(), "a.tar.gz".to_string()]);
}

#[test]
fn backups_to_purge_keep_superieur_au_total_ne_purge_rien() {
    let all = vec!["b.tar.gz".to_string(), "a.tar.gz".to_string()];
    assert!(backups_to_purge(all, 10).is_empty());
}

#[test]
fn backups_to_purge_liste_vide_donne_liste_vide() {
    assert!(backups_to_purge(vec![], 5).is_empty());
}
