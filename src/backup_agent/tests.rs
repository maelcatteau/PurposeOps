//! Tests de la partie pure de `backup_agent` — l'orchestration (SSH, compilation, etc.)
//! n'est vérifiable qu'en direct (voir tests/backup_agent_workflow.py).

use super::*;
use crate::config::{CustomerHost, DbCredentials, DeploymentHost};

fn sample_host() -> Host {
    Host {
        name: "vps-mcm".to_string(),
        hostname: "46.202.131.25".to_string(),
        user: "ngner".to_string(),
        port: "2222".to_string(),
        identity_file: "/etc/ssh/mcm".to_string(),
        arch: "x86_64".to_string(),
        docker_context: "remote-vps-mcm".to_string(),
        identity_key: Some("enc:abcd".to_string()),
    }
}

fn sample_deployment() -> Deployment {
    Deployment {
        service_name: "Odoo CE".to_string(),
        hosts: vec![DeploymentHost {
            host_id: "mcm".to_string(),
            path_for_service: "/home/ngner/odoo-perso".to_string(),
            path_for_docker_compose: "/home/ngner/odoo-perso/docker-compose.yml".to_string(),
        }],
        deployment_id: "odoo-perso".to_string(),
        container_name: Some("odoo-prod".to_string()),
        db_container_name: Some("odoo-db-prod".to_string()),
        database_name: Some("catteaumael".to_string()),
        db_credentials: Some(DbCredentials {
            host: "odoo-db-prod".to_string(),
            port: "5432".to_string(),
            user: "odoo".to_string(),
            password: "enc:xyz".to_string(),
        }),
    }
}

fn sample_customer(dep: &Deployment) -> Customer {
    Customer {
        abbreviation: "moi".to_string(),
        deployments: vec![dep.clone()],
        hosts: vec![CustomerHost {
            host_id: "mcm".to_string(),
            path_on_host: "/home/ngner".to_string(),
        }],
    }
}

#[test]
fn build_scoped_host_force_localhost_et_vide_les_champs_ssh() {
    let scoped = build_scoped_host(&sample_host());
    assert_eq!(scoped.hostname, "localhost");
    assert_eq!(scoped.port, "");
    assert_eq!(scoped.identity_file, "");
    assert_eq!(scoped.identity_key, None);
    assert_eq!(scoped.arch, "x86_64");
    assert_eq!(scoped.docker_context, "default");
    assert_eq!(scoped.name, "vps-mcm");
    assert_eq!(scoped.user, "ngner");
}

#[test]
fn build_scoped_customers_ne_garde_que_le_deploiement_cible() {
    let dep = sample_deployment();
    let mut other = sample_deployment();
    other.deployment_id = "autre-deploiement".to_string();
    let mut customer = sample_customer(&dep);
    customer.deployments.push(other);

    let scoped = build_scoped_customers("Mael", &customer, &dep);
    assert_eq!(scoped.len(), 1);
    let scoped_customer = &scoped["Mael"];
    assert_eq!(scoped_customer.deployments.len(), 1);
    assert_eq!(scoped_customer.deployments[0].deployment_id, "odoo-perso");
    assert_eq!(scoped_customer.abbreviation, "moi");
}

#[test]
fn build_scoped_context_preselectionne_le_deploiement() {
    let dep = sample_deployment();
    let customer = sample_customer(&dep);
    let scoped_host = build_scoped_host(&sample_host());
    let ctx = build_scoped_context("mcm", &scoped_host, "Mael", &customer, &dep);

    assert_eq!(ctx.host["mcm"].hostname, "localhost");
    assert_eq!(ctx.customer["Mael"].abbreviation, "moi");
    assert!(!ctx.prompt_show);
    match ctx.deployment {
        Some(DeploymentField::Full(d)) => assert_eq!(d.deployment_id, "odoo-perso"),
        other => panic!("attendu DeploymentField::Full, obtenu {other:?}"),
    }
}

#[test]
fn build_cron_line_contient_les_elements_attendus() {
    let line = build_cron_line("odoo-perso", "ngner", "/opt/ppo/ppo", Some("https://ntfy.sh/topic"), 10);
    assert!(line.contains("NTFY_URL=https://ntfy.sh/topic\n"));
    assert!(line.contains("ngner /opt/ppo/ppo backup run --cron --keep-last 10"));
    assert!(line.contains("odoo-perso"));
}

#[test]
fn build_cron_line_sans_ntfy_omet_la_ligne() {
    let line = build_cron_line("odoo-perso", "ngner", "/opt/ppo/ppo", None, 10);
    assert!(!line.contains("NTFY_URL"));
}

#[test]
fn find_deployment_globally_cherche_tous_les_clients() {
    let dep = sample_deployment();
    let customer = sample_customer(&dep);
    let customers = BTreeMap::from([("Mael".to_string(), customer)]);

    let found = find_deployment_globally("odoo-perso", &customers);
    assert!(found.is_some());
    let (name, _, found_dep) = found.unwrap();
    assert_eq!(name, "Mael");
    assert_eq!(found_dep.deployment_id, "odoo-perso");

    assert!(find_deployment_globally("inconnu", &customers).is_none());
}

#[test]
fn host_matches_local_arch_mappe_arm64_vers_aarch64() {
    // Le nom "local" réel dépend de la machine qui lance les tests ; on ne teste que la
    // logique de correspondance elle-même, pas une valeur figée.
    let local = std::env::consts::ARCH;
    assert!(host_matches_local_arch(local));
    if local == "aarch64" {
        assert!(host_matches_local_arch("arm64"));
    } else {
        assert!(!host_matches_local_arch("arm64"));
    }
    assert!(!host_matches_local_arch("some-made-up-arch"));
}
