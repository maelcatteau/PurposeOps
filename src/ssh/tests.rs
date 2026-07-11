//! Tests SSH. Les fonctions pures (parsing, résolution de chemin) sont couvertes par
//! `cargo test`. Le test réseau réel est marqué `#[ignore]` — pas de suite automatisée
//! pour ce qui touche l'infra (convention du projet), on le lance à la main :
//! `cargo test -- --ignored live_`.

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
