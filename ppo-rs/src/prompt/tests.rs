//! Tests de la logique de prompt (les 6 cas du code nu + l'ancien format string).
//! Convention projet : fichier frère, pas de bloc `#[cfg(test)]` inline dans prompt.rs.

use super::format_prompt;
use crate::config::Context;

/// Construit un contexte de test à partir d'un extrait YAML (double comme test de parsing).
fn ctx(yaml: &str) -> Context {
    serde_yaml_ng::from_str(yaml).expect("YAML de test invalide")
}

const HOST_REMOTE: &str = "  mcm: {name: vps-mcm, hostname: 46.202.131.25, user: ngner, port: '2222', identity_file: /etc/ssh/mcm, arch: x86_64, docker_context: remote-vps-mcm}";
const HOST_LOCAL: &str = "  localhost: {name: Local Machine, hostname: localhost, user: ngner, port: '', identity_file: '', arch: x86_64, docker_context: default}";

fn build(host: &str, prompt_show: bool, customer: &str, deployment: &str) -> Context {
    ctx(&format!(
        "host:\n{host}\nprompt_show: {prompt_show}\ncustomer: {customer}\ndeployment: {deployment}"
    ))
}

#[test]
fn masque_si_prompt_show_false() {
    let c = build(HOST_REMOTE, false, "{}", "null");
    assert_eq!(format_prompt(&c), "");
}

#[test]
fn local_sans_client() {
    let c = build(HOST_LOCAL, true, "{}", "null");
    assert_eq!(format_prompt(&c), "🏠 local");
}

#[test]
fn distant_sans_client() {
    let c = build(HOST_REMOTE, true, "{}", "null");
    assert_eq!(format_prompt(&c), "🌐 vps-mcm");
}

#[test]
fn local_avec_client_sans_deploiement() {
    let c = build(HOST_LOCAL, true, "{Mael: {abbreviation: moi}}", "null");
    assert_eq!(format_prompt(&c), "🏠 local - moi");
}

#[test]
fn distant_avec_client_et_deploiement_complet() {
    let deployment = "{service_name: Odoo CE, hosts: [], deployment_id: odoo-perso}";
    let c = build(HOST_REMOTE, true, "{Mael: {abbreviation: moi}}", deployment);
    assert_eq!(format_prompt(&c), "🌐 vps-mcm - moi (odoo-perso)");
}

#[test]
fn ancien_format_string_pas_de_suffixe() {
    // Un déploiement en ancien format (string) ne doit PAS produire de suffixe.
    let c = build(HOST_REMOTE, true, "{Mael: {abbreviation: moi}}", "odoo-perso");
    assert_eq!(format_prompt(&c), "🌐 vps-mcm - moi");
}

#[test]
fn aucun_hote_donne_unknown() {
    let c = ctx("host: {}\nprompt_show: true\ncustomer: {}\ndeployment: null");
    assert_eq!(format_prompt(&c), "❓ unknown");
}
