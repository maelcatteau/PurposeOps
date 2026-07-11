//! Tests SSH. Les fonctions pures (parsing, résolution de chemin) sont couvertes par
//! `cargo test`. Depuis l'introduction du seam `spawn`/`install_test_runner`, les
//! fonctions qui lancent réellement `ssh` sont elles aussi couvertes, sans toucher au
//! réseau (voir PORTING.md). Le test réseau réel reste marqué `#[ignore]` — pas de
//! suite automatisée pour ce qui touche l'infra pour de vrai, on le lance à la main :
//! `cargo test -- --ignored live_`.

use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn parse_nom_de_socket_valide() {
    let (user, host, port) = parse_socket_name("ngner@46.202.131.25:2222").unwrap();
    assert_eq!(user, "ngner");
    assert_eq!(host, "46.202.131.25");
    assert_eq!(port, "2222");
}

#[test]
fn parse_nom_de_socket_invalide() {
    assert!(parse_socket_name("pas-un-socket").is_none());
}

#[test]
fn resolve_key_path_tilde() {
    let home = std::env::var("HOME").unwrap();
    assert_eq!(resolve_key_path("~/foo/bar"), format!("{home}/foo/bar"));
}

#[test]
fn resolve_key_path_absolu_inchange() {
    assert_eq!(resolve_key_path("/etc/ssh/mcm"), "/etc/ssh/mcm");
}

#[test]
fn resolve_remote_path_remplace_le_tilde() {
    assert_eq!(resolve_remote_path("~/backups/moi/mcm", "ngner"), "/home/ngner/backups/moi/mcm");
}

#[test]
fn resolve_remote_path_utilise_l_utilisateur_donne() {
    assert_eq!(resolve_remote_path("~/backups/moi/mcm", "ppo"), "/home/ppo/backups/moi/mcm");
}

#[test]
fn resolve_remote_path_chemin_absolu_inchange() {
    assert_eq!(resolve_remote_path("/srv/backups", "ngner"), "/srv/backups");
}

#[test]
fn resolve_key_path_vide_inchange() {
    assert_eq!(resolve_key_path(""), "");
}

fn sample_host() -> Host {
    Host {
        name: "vps-mcm".to_string(),
        hostname: "46.202.131.25".to_string(),
        user: "ngner".to_string(),
        port: "2222".to_string(),
        identity_file: "/etc/ssh/mcm".to_string(),
        arch: "x86_64".to_string(),
        docker_context: "remote-vps-mcm".to_string(),
        identity_key: None,
    }
}

#[test]
fn ssh_target_format_user_arobase_hostname() {
    assert_eq!(ssh_target(&sample_host()), "ngner@46.202.131.25");
}

#[test]
fn host_from_socket_name_valide_reconstruit_le_host() {
    let host = host_from_socket_name("ngner@46.202.131.25:2222").unwrap();
    assert_eq!(host.name, "46.202.131.25");
    assert_eq!(host.hostname, "46.202.131.25");
    assert_eq!(host.user, "ngner");
    assert_eq!(host.port, "2222");
    assert_eq!(host.identity_file, "");
    assert_eq!(host.arch, "");
    assert_eq!(host.docker_context, "");
    assert!(host.identity_key.is_none());
}

#[test]
fn host_from_socket_name_invalide_donne_none() {
    assert!(host_from_socket_name("pas-un-socket").is_none());
}

/// Hôte fictif, jamais réel : hostname/user/port choisis pour qu'aucun socket
/// `controlmasters/` existant ne puisse coïncider (ces tests ne dépendent d'aucun
/// vrai processus réseau une fois un faux exécuteur installé, mais un nom d'hôte
/// clairement bidon évite toute ambiguïté à la lecture).
fn unreachable_test_host() -> Host {
    Host {
        name: "test-unreachable".to_string(),
        hostname: "test-host-unreachable.invalid".to_string(),
        user: "test-user-seam".to_string(),
        port: "1".to_string(),
        identity_file: String::new(),
        arch: String::new(),
        docker_context: String::new(),
        identity_key: None,
    }
}

#[test]
fn run_with_master_echappe_les_doubles_accolades() {
    let host = unreachable_test_host();
    let _socket = with_fake_socket(&host);
    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured2 = captured.clone();
    let _g = install_connected_test_runner(move |program, args| {
        assert_eq!(program, "ssh");
        captured2.lock().unwrap().push(args.last().unwrap().clone());
        Ok(fake_output(0, "ok", ""))
    });

    let out = run_with_master(&host, "echo {{foo}} et }}bar{{").unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "ok");

    let sent = captured.lock().unwrap();
    let last_sent = sent.last().expect("au moins un appel ssh attendu");
    assert_eq!(last_sent, "echo \\{\\{foo\\}\\} et \\}\\}bar\\{\\{");
}

/// Vrai sleep de 500ms dans `create_master_connection` (délibérément pas fake-cloqué,
/// voir PORTING.md) : ce test prend donc un peu de temps pour de vraies raisons, pas
/// un oubli.
#[test]
fn run_with_master_echoue_si_la_connexion_ne_peut_jamais_etre_etablie() {
    let host = unreachable_test_host();
    let _g = install_test_runner(|program, _args| {
        assert_eq!(program, "ssh");
        Ok(fake_output(255, "", "Connection refused"))
    });

    let err = run_with_master(&host, "uptime").unwrap_err();
    assert!(err.to_string().contains("Failed to establish master connection"));
}

/// Preuve live de la Phase 3.1 : exécute `uptime` sur mcm en réutilisant (ou créant)
/// la connexion maître partagée avec le côté nu. Lancer manuellement :
/// `cargo test -- --ignored live_run_with_master_uptime_on_mcm`.
#[test]
#[ignore]
fn live_run_with_master_uptime_on_mcm() {
    let hosts = crate::config::load_hosts().unwrap();
    let mcm = hosts.get("mcm").expect("hôte 'mcm' absent de hosts.yaml");
    let output = run_with_master(mcm, "uptime").unwrap();
    assert!(output.status.success(), "ssh a échoué : {output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("uptime sur mcm : {stdout}");
    assert!(!stdout.trim().is_empty());
}
