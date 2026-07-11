use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};

use super::*;
use crate::ssh::fake_output as output;

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

/// Hôte `localhost` : route `exec_remote`/`exec_remote_shell` toutes les deux vers
/// `ssh::spawn` (`docker`/`sh`), sans toucher au vrai `controlmasters/`.
fn localhost_host() -> Host {
    Host {
        name: "localhost".to_string(),
        hostname: "localhost".to_string(),
        user: String::new(),
        port: String::new(),
        identity_file: String::new(),
        arch: String::new(),
        docker_context: String::new(),
        identity_key: None,
    }
}

#[test]
fn list_remote_backups_filtre_et_conserve_l_ordre() {
    let host = localhost_host();
    let _g = ssh::install_test_runner(|program, args| {
        assert_eq!(program, "sh");
        assert!(args.last().unwrap().starts_with("ls -1t"));
        Ok(ssh::fake_output(0, "c.tar.gz\nnotes.txt\nb.tar.gz\na.tar.gz\n", ""))
    });

    let backups = list_remote_backups("/srv/backups", &host).unwrap();
    assert_eq!(backups, vec!["c.tar.gz", "b.tar.gz", "a.tar.gz"]);
}

#[test]
fn purge_old_backups_supprime_uniquement_ce_qui_depasse_keep() {
    let host = localhost_host();
    let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let calls2 = calls.clone();
    let _g = ssh::install_test_runner(move |program, args| {
        assert_eq!(program, "sh");
        let cmd = args.last().unwrap().clone();
        calls2.lock().unwrap().push(cmd.clone());
        if cmd.starts_with("ls -1t") {
            Ok(ssh::fake_output(0, "c.tar.gz\nb.tar.gz\na.tar.gz\n", ""))
        } else {
            Ok(ssh::fake_output(0, "", ""))
        }
    });

    purge_old_backups("/srv/backups", &host, 1).unwrap();

    let calls = calls.lock().unwrap();
    assert!(calls.iter().any(|c| c.contains("rm -f '/srv/backups/b.tar.gz'")));
    assert!(calls.iter().any(|c| c.contains("rm -f '/srv/backups/a.tar.gz'")));
    assert!(!calls.iter().any(|c| c.contains("rm -f '/srv/backups/c.tar.gz'")));
}

#[test]
fn purge_old_backups_ne_supprime_rien_si_rien_a_purger() {
    let host = localhost_host();
    let _g = ssh::install_test_runner(|program, args| {
        assert_eq!(program, "sh");
        let cmd = args.last().unwrap();
        assert!(!cmd.starts_with("rm -f"), "aucun rm attendu : {cmd}");
        Ok(ssh::fake_output(0, "a.tar.gz\n", ""))
    });

    purge_old_backups("/srv/backups", &host, 5).unwrap();
}

/// Un échec de `pg_dump` doit déclencher le nettoyage best-effort (rm côté conteneur DB,
/// conteneur APP, et hôte) puis propager l'erreur d'origine — pas la masquer.
#[test]
fn do_generic_backup_nettoie_et_propage_l_erreur_si_pg_dump_echoue() {
    let host = localhost_host();
    type Call = (String, Vec<String>);
    let calls: Arc<Mutex<Vec<Call>>> = Arc::new(Mutex::new(Vec::new()));
    let calls2 = calls.clone();
    let _g = ssh::install_test_runner(move |program, args| {
        calls2.lock().unwrap().push((program.to_string(), args.to_vec()));
        if program == "docker" && args.iter().any(|a| a == "pg_dump") {
            return Ok(ssh::fake_output(1, "", "pg_dump: erreur de connexion"));
        }
        Ok(ssh::fake_output(0, "ok", ""))
    });

    let err = do_generic_backup(
        "ma_base", "app", "db", &host, "5432", "odoo", "secret", "/srv/backups", false, None,
    )
    .unwrap_err();

    assert!(err.to_string().contains("dump SQL (pg_dump)"));

    let calls = calls.lock().unwrap();
    assert!(
        calls.iter().any(|(p, a)| p == "docker" && a.iter().any(|x| x == "rm")),
        "nettoyage docker (rm) attendu après l'échec : {calls:?}"
    );
    assert!(
        calls.iter().any(|(p, a)| p == "sh" && a.iter().any(|x| x.starts_with("rm -f"))),
        "nettoyage shell (rm -f) attendu après l'échec : {calls:?}"
    );
}

/// Un `keep_last` fourni déclenche la purge après un backup réussi, best-effort : une
/// purge qui échoue ne doit pas transformer un backup réussi en erreur.
#[test]
fn do_generic_backup_purge_best_effort_n_invalide_pas_un_backup_reussi() {
    let host = localhost_host();
    let _g = ssh::install_test_runner(|program, args| {
        if program == "sh" && args.last().unwrap().starts_with("ls -1t") {
            // `list_remote_backups` ne regarde que le code de sortie via `?` sur
            // l'échec de spawn lui-même, pas le statut de sortie de la commande : un
            // vrai I/O error (pas juste un exit code non nul) est nécessaire ici pour
            // que `purge_old_backups` échoue réellement et exerce le chemin best-effort.
            return Err(std::io::Error::other("boom : ssh indisponible"));
        }
        Ok(ssh::fake_output(0, "ok", ""))
    });

    let result = do_generic_backup(
        "ma_base", "app", "db", &host, "5432", "odoo", "secret", "/srv/backups", false, Some(1),
    );
    assert!(result.is_ok(), "la purge en échec ne doit pas invalider le backup : {result:?}");
}
