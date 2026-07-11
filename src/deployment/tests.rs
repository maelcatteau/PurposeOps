use super::*;

fn sample_deployment(id: &str, host_id: &str) -> Deployment {
    Deployment {
        service_name: "Odoo CE".to_string(),
        hosts: vec![DeploymentHost {
            host_id: host_id.to_string(),
            path_for_service: format!("/home/ngner/{id}"),
            path_for_docker_compose: format!("/home/ngner/{id}/docker-compose.yml"),
        }],
        deployment_id: id.to_string(),
        container_name: None,
        db_container_name: None,
        database_name: None,
        db_credentials: None,
    }
}

fn sample_customer(deployments: Vec<Deployment>) -> Customer {
    Customer { abbreviation: "moi".to_string(), deployments, hosts: vec![] }
}

#[test]
fn host_for_deployment_trouve_a_travers_plusieurs_clients() {
    let customers = BTreeMap::from([
        ("Mael".to_string(), sample_customer(vec![sample_deployment("odoo-perso", "mcm")])),
        ("Sylvie".to_string(), sample_customer(vec![sample_deployment("odoo-sylvie", "ngner")])),
    ]);

    assert_eq!(host_for_deployment("odoo-sylvie", &customers), Some("ngner".to_string()));
    assert_eq!(host_for_deployment("odoo-perso", &customers), Some("mcm".to_string()));
}

#[test]
fn host_for_deployment_introuvable_donne_none() {
    let customers = BTreeMap::from([(
        "Mael".to_string(),
        sample_customer(vec![sample_deployment("odoo-perso", "mcm")]),
    )]);
    assert_eq!(host_for_deployment("inexistant", &customers), None);
}

#[test]
fn host_for_deployment_deploiement_sans_hote_donne_none() {
    let mut dep = sample_deployment("sans-hote", "mcm");
    dep.hosts.clear();
    let customers =
        BTreeMap::from([("Mael".to_string(), sample_customer(vec![dep]))]);
    assert_eq!(host_for_deployment("sans-hote", &customers), None);
}

#[test]
fn deployment_id_exists_vrai_si_present_chez_nimporte_quel_client() {
    let customers = BTreeMap::from([
        ("Mael".to_string(), sample_customer(vec![sample_deployment("odoo-perso", "mcm")])),
        ("Sylvie".to_string(), sample_customer(vec![sample_deployment("odoo-sylvie", "ngner")])),
    ]);
    assert!(deployment_id_exists("odoo-sylvie", &customers));
    assert!(deployment_id_exists("odoo-perso", &customers));
}

#[test]
fn deployment_id_exists_faux_si_absent() {
    let customers = BTreeMap::from([(
        "Mael".to_string(),
        sample_customer(vec![sample_deployment("odoo-perso", "mcm")]),
    )]);
    assert!(!deployment_id_exists("inconnu", &customers));
}

#[test]
fn deployment_id_exists_config_vide_donne_faux() {
    let customers: BTreeMap<String, Customer> = BTreeMap::new();
    assert!(!deployment_id_exists("peu-importe", &customers));
}
