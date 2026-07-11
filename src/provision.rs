//! `provision` — déploiement de bout en bout d'un nouveau service (Phase 9.2).
//!
//! **Capacité entièrement nouvelle** : il n'existe aucun équivalent côté nu, même
//! partiel — `templater.nu` rend un compose en local sans jamais le pousser nulle part,
//! `docker-compose-functions.nu` ne pilote que des piles *déjà* connues de `docker ps`,
//! et `deployment-manager/core.nu` enregistre des métadonnées sans toucher à SSH/Docker.
//! Il n'existe pas non plus de mécanisme de transfert de fichier (`scp`/`rsync`) dans le
//! projet — le compose rendu est poussé en l'encodant en base64 dans une commande shell
//! distante envoyée via la connexion ControlMaster déjà en place (`ssh::run_with_master`),
//! plutôt que d'ouvrir une connexion `scp` séparée.

use std::path::Path;

use anyhow::{Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::config::{self, Deployment, DeploymentHost, Host};
use crate::{customer, deployment, docker, ssh, template, ui};

/// Pousse `content` vers `remote_path` sur `host`. Écriture directe pour `localhost` ;
/// pour un hôte distant, encode en base64 et l'écrit via une commande shell sur la
/// connexion ControlMaster existante — il n'y a pas d'autre mécanisme de transfert de
/// fichier dans ce projet (pas de `scp`/`rsync`).
fn push_file(host: &Host, remote_path: &str, content: &str) -> Result<()> {
    if let Some(parent) = Path::new(remote_path).parent() {
        let parent = parent.display();
        ssh::exec_shell_checked(host, &format!("mkdir -p '{parent}'"), "création du dossier parent")?;
    }

    if host.hostname == "localhost" {
        std::fs::write(remote_path, content)?;
        return Ok(());
    }

    let encoded = BASE64.encode(content.as_bytes());
    let cmd = format!("echo '{encoded}' | base64 -d > '{remote_path}'");
    ssh::exec_shell_checked(host, &cmd, "envoi du docker-compose.yml")?;
    Ok(())
}

/// `provision` — wizard interactif : rendu du template → aperçu compose + déploiement →
/// confirmation unique (couvre la création du dossier distant, l'envoi du compose ET le
/// `docker compose up -d`, pas de deuxième confirmation juste avant `up`) → exécution →
/// enregistrement du déploiement dans `customers.yaml` (même mécanisme que `cdep`, y
/// compris le chiffrement de la clé SSH de l'hôte pour ce client). Ne gère pas les champs
/// DB (`db_credentials`...) : les services actuellement templatés (Vaultwarden, Caddy)
/// n'en ont pas ; `cdep` reste la voie pour un déploiement avec base de données.
pub fn cmd_provision() -> Result<()> {
    let (customer_name, _) = customer::get_current_customer()?
        .ok_or_else(|| anyhow!("Aucun client sélectionné. Utilisez 'sc <client>' d'abord."))?;

    let services = config::load_services()?;
    if services.is_empty() {
        println!("(aucun service disponible)");
        return Ok(());
    }
    let Some(service_name) = ui::select("Service à déployer :", services.keys().cloned().collect())
    else {
        return Ok(());
    };

    let mut hosts = config::load_hosts()?;
    let Some(host_id) = ui::select("Host ID :", hosts.keys().cloned().collect()) else {
        return Ok(());
    };
    let host = hosts
        .get(&host_id)
        .cloned()
        .ok_or_else(|| anyhow!("Hôte '{host_id}' introuvable"))?;

    let Some(docker_service_name) = ui::text("Nom du service Docker pour cette instance : ") else {
        return Ok(());
    };
    let Some(path_for_service) = ui::text("Chemin du service sur l'hôte : ") else {
        return Ok(());
    };
    let Some(path_for_docker_compose) = ui::text("Chemin du fichier docker-compose.yml sur l'hôte : ")
    else {
        return Ok(());
    };
    let Some(deployment_id) = ui::text("Deployment id (unique) : ") else {
        return Ok(());
    };

    let mut customers = config::load_customers()?;
    if deployment::deployment_id_exists(&deployment_id, &customers) {
        bail!("Le deployment_id '{deployment_id}' est déjà utilisé par un autre déploiement.");
    }

    let Some(compose) = template::generate_compose(&service_name, &docker_service_name)? else {
        println!("❌ Rendu annulé");
        return Ok(());
    };

    let new_deployment = Deployment {
        service_name: service_name.clone(),
        hosts: vec![DeploymentHost {
            host_id: host_id.clone(),
            path_for_service: path_for_service.clone(),
            path_for_docker_compose: path_for_docker_compose.clone(),
        }],
        deployment_id: deployment_id.clone(),
        container_name: None,
        db_container_name: None,
        database_name: None,
        db_credentials: None,
    };

    println!("--- docker-compose.yml rendu ---");
    println!("{compose}");
    println!("--- Déploiement à enregistrer ---");
    println!("{}", serde_yaml_ng::to_string(&new_deployment)?);

    if !ui::confirm("Provisionner ce service (créer le dossier, envoyer le compose, docker compose up -d) ?")
    {
        println!("❌ Opération annulée");
        return Ok(());
    }

    println!("📁 Création du dossier distant si nécessaire ({path_for_service})...");
    ssh::exec_shell_checked(
        &host,
        &format!("mkdir -p '{path_for_service}'"),
        "création du dossier distant",
    )?;

    println!("📤 Envoi du docker-compose.yml vers {path_for_docker_compose}...");
    push_file(&host, &path_for_docker_compose, &compose)?;

    println!("🚀 docker compose up -d...");
    let output = docker::run_docker_command(&["compose", "-f", &path_for_docker_compose, "up", "-d"], &host)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Échec de 'docker compose up -d' : {}", stderr.trim());
    }

    let cust = customers
        .get_mut(&customer_name)
        .ok_or_else(|| anyhow!("Client '{customer_name}' introuvable"))?;
    cust.deployments.push(new_deployment);
    config::save_yaml_map(&config::customers_config_path(), &customers)?;

    if deployment::ensure_host_key_encrypted(&host_id, &mut hosts, &customers) {
        config::save_yaml_map(&config::hosts_config_path(), &hosts)?;
    }

    println!("✅ Service '{deployment_id}' provisionné pour '{customer_name}' sur '{host_id}'");
    Ok(())
}
