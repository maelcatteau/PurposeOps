//! Tests de la couche config : parsing des variantes de `deployment` + round-trip.
//! (Convention projet : les tests vivent dans ce fichier frère, pas inline dans config.rs.)

use super::*;

/// Un contexte complet représentatif du context.yaml réel.
const FULL_CONTEXT: &str = "
host:
  mcm:
    name: vps-mcm
    hostname: 46.202.131.25
    user: ngner
    port: '2222'
    identity_file: /etc/ssh/mcm
    arch: x86_64
    docker_context: remote-vps-mcm
prompt_show: false
customer:
  Mael:
    abbreviation: moi
deployment:
  service_name: Odoo CE
  hosts:
  - host_id: mcm
    path_for_service: /home/ngner/odoo-perso
    path_for_docker_compose: /home/ngner/odoo-perso/docker-compose.yml
  deployment_id: odoo-perso
  container_name: odoo-prod-mael
  db_container_name: odoo-db-prod-mael
  database_name: catteaumael
  db_credentials:
    host: db-prod-mael
    port: '5432'
    user: odoo
    password: odoo_password_2025
";

#[test]
fn parse_contexte_reel() {
    let ctx: Context = serde_yaml_ng::from_str(FULL_CONTEXT).unwrap();
    assert_eq!(ctx.host.len(), 1);
    assert_eq!(ctx.host["mcm"].name, "vps-mcm");
    assert_eq!(ctx.host["mcm"].port, "2222"); // reste une String
    assert_eq!(ctx.customer["Mael"].abbreviation, "moi");
    match ctx.deployment {
        Some(DeploymentField::Full(d)) => assert_eq!(d.deployment_id, "odoo-perso"),
        _ => panic!("attendu un déploiement Full"),
    }
}

#[test]
fn deployment_null_donne_none() {
    let yaml = "
host:
  localhost: {name: Local Machine, hostname: localhost, user: ngner, port: '', identity_file: '', arch: x86_64, docker_context: default}
prompt_show: true
customer: {}
deployment: null
";
    let ctx: Context = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(ctx.deployment.is_none());
    assert!(ctx.customer.is_empty());
}

#[test]
fn deployment_ancien_format_string() {
    let yaml = "
host:
  localhost: {name: Local Machine, hostname: localhost, user: ngner, port: '', identity_file: '', arch: x86_64, docker_context: default}
prompt_show: true
customer: {}
deployment: odoo-perso
";
    let ctx: Context = serde_yaml_ng::from_str(yaml).unwrap();
    match ctx.deployment {
        Some(DeploymentField::Legacy(s)) => assert_eq!(s, "odoo-perso"),
        _ => panic!("attendu un déploiement Legacy (string)"),
    }
}

#[test]
fn parse_customers_avec_deployments_vides_et_db_optionnelle() {
    // Multibikes : deployments []. Sylvie : déploiement avec champs DB. Vaultwarden : sans DB.
    let yaml = "
Multibikes:
  abbreviation: mb
  deployments: []
  hosts:
  - host_id: mcm
    path_on_host: /home/ngner/multibikes/
Cocotte:
  abbreviation: cocotte
  deployments:
  - service_name: Vaultwarden
    hosts:
    - host_id: ngner
      path_for_service: /home/ngner/vw/
      path_for_docker_compose: /home/ngner/vw/compose.yaml
    deployment_id: ngner-cocotte-Vaultwarden
  hosts:
  - host_id: ngner
    path_on_host: /home/ngner/cocotte/
Sylvie:
  abbreviation: Syl
  deployments:
  - service_name: Odoo CE
    hosts:
    - host_id: ngner
      path_for_service: /home/ngner/odoo-sylvie/
      path_for_docker_compose: /home/ngner/odoo-sylvie/docker-compose.yml
    deployment_id: odoo-prod-sylvie
    container_name: odoo-prod-sylvie
    db_container_name: odoo-prod-sylvie-db
    database_name: Feijoasis
    db_credentials:
      host: odoo-prod-sylvie-db
      port: '5432'
      user: odoo
      password: secret
  hosts:
  - host_id: ngner
    path_on_host: /home/ngner/odoo-sylvie
";
    let customers: BTreeMap<String, Customer> = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(customers["Multibikes"].deployments.is_empty());
    // Service sans DB : les champs optionnels sont None.
    let vw = &customers["Cocotte"].deployments[0];
    assert!(vw.db_credentials.is_none());
    assert!(vw.container_name.is_none());
    // Service avec DB : présents.
    let odoo = &customers["Sylvie"].deployments[0];
    assert_eq!(odoo.database_name.as_deref(), Some("Feijoasis"));
    assert_eq!(odoo.db_credentials.as_ref().unwrap().port, "5432");
}

#[test]
fn parse_services() {
    let yaml = "
Vaultwarden:
  template_dir_path: ~/templates/Vaultwarden/
  template_compose_path: ~/templates/Vaultwarden/docker-compose.yml
  variables: []
";
    let services: BTreeMap<String, Service> = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(services.contains_key("Vaultwarden"));
    assert!(services["Vaultwarden"].variables.is_empty());
}

#[test]
fn round_trip_preserve_les_champs() {
    let ctx: Context = serde_yaml_ng::from_str(FULL_CONTEXT).unwrap();
    let dumped = serde_yaml_ng::to_string(&ctx).unwrap();
    let reparsed: Context = serde_yaml_ng::from_str(&dumped).unwrap();

    // Le mot de passe DB (le champ le plus critique à ne pas perdre au round-trip) survit.
    let creds = match reparsed.deployment {
        Some(DeploymentField::Full(d)) => d.db_credentials.unwrap(),
        _ => panic!("déploiement perdu au round-trip"),
    };
    assert_eq!(creds.password, "odoo_password_2025");
    assert_eq!(reparsed.host["mcm"].docker_context, "remote-vps-mcm");
}
