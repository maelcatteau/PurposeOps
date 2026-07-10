//! Tests de la validation de cohérence (chemin vert + rouge), sans toucher aux vrais YAML.

use super::find_problems;
use crate::config::{Context, Customer, Host};
use std::collections::BTreeMap;

fn hosts(yaml: &str) -> BTreeMap<String, Host> {
    serde_yaml_ng::from_str(yaml).unwrap()
}
fn customers(yaml: &str) -> BTreeMap<String, Customer> {
    serde_yaml_ng::from_str(yaml).unwrap()
}
fn empty_ctx() -> Context {
    serde_yaml_ng::from_str("host: {}\nprompt_show: true\ncustomer: {}\ndeployment: null").unwrap()
}

const HOSTS: &str = "
ngner: {name: vps-ngner, hostname: h, user: u, port: '', identity_file: '', arch: x, docker_context: d}
mcm:   {name: vps-mcm,   hostname: h, user: u, port: '', identity_file: '', arch: x, docker_context: d}
";

#[test]
fn config_coherente_aucune_erreur() {
    let c = customers(
        "
A:
  abbreviation: a
  deployments:
  - service_name: Odoo CE
    hosts: [{host_id: ngner, path_for_service: /x, path_for_docker_compose: /x/c.yml}]
    deployment_id: dep-a
  hosts: [{host_id: ngner, path_on_host: /x}]
",
    );
    assert!(find_problems(&hosts(HOSTS), &c, &empty_ctx()).is_empty());
}

#[test]
fn host_id_inconnu_est_signale() {
    let c = customers(
        "
A:
  abbreviation: a
  deployments: []
  hosts: [{host_id: nexistepas, path_on_host: /x}]
",
    );
    let problems = find_problems(&hosts(HOSTS), &c, &empty_ctx());
    assert!(
        problems.iter().any(|p| p.contains("hôte inconnu 'nexistepas'")),
        "attendu un signalement d'hôte inconnu, obtenu : {problems:?}"
    );
}

#[test]
fn deployment_id_duplique_est_signale() {
    let c = customers(
        "
A:
  abbreviation: a
  deployments:
  - {service_name: S, hosts: [{host_id: ngner, path_for_service: /x, path_for_docker_compose: /x/c.yml}], deployment_id: partage}
  hosts: [{host_id: ngner, path_on_host: /x}]
B:
  abbreviation: b
  deployments:
  - {service_name: S, hosts: [{host_id: mcm, path_for_service: /y, path_for_docker_compose: /y/c.yml}], deployment_id: partage}
  hosts: [{host_id: mcm, path_on_host: /y}]
",
    );
    let problems = find_problems(&hosts(HOSTS), &c, &empty_ctx());
    assert!(
        problems.iter().any(|p| p.contains("'partage' dupliqué")),
        "attendu un signalement de doublon, obtenu : {problems:?}"
    );
}
