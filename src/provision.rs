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

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::config::{self, Deployment, DeploymentHost, Host};
use crate::{customer, deployment, docker, ssh, template, ui};

/// Pousse `content` vers `remote_path` sur `host`. Écriture directe pour `localhost` ;
/// pour un hôte distant, encode en base64 et l'écrit via une commande shell sur la
/// connexion ControlMaster existante — il n'y a pas d'autre mécanisme de transfert de
/// fichier dans ce projet (pas de `scp`/`rsync`). Réutilisé par `backup_agent.rs` pour
/// pousser les YAML/identité `age` scopés d'un agent de backup (petits fichiers texte,
/// ce chemin reste adapté ; voir `push_binary` pour un exécutable compilé).
pub(crate) fn push_file(host: &Host, remote_path: &str, content: &str) -> Result<()> {
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
    ssh::exec_shell_checked(host, &cmd, "envoi du fichier")?;
    Ok(())
}

/// Taille de bloc pour `push_binary`, en octets bruts (avant expansion base64 ~1.37x).
/// Linux plafonne un unique argument/variable d'environnement à `MAX_ARG_STRLEN`
/// (`32 * PAGE_SIZE` = 128 KiB), indépendamment du plus grand `ARG_MAX` (2 MiB) qui limite
/// la somme argv+envp — c'est cette première limite, sur UNE SEULE chaîne contiguë, que le
/// `echo '<b64>' | base64 -d > path` de `push_file` heurterait sur un exécutable de
/// plusieurs Mo (mesuré : ~4.8 Mo une fois strippé, encodé ça dépasserait 6 Mo en un seul
/// argument). 64 KiB bruts (~87 KiB encodés + habillage shell) laisse une marge confortable
/// sous ce plafond. Valeur de départ dérivée du calcul de la contrainte, pas encore
/// validée en conditions réelles contre un VPS distant (voir PORTING.md Phase 11.2).
const BINARY_CHUNK_SIZE: usize = 64 * 1024;

fn sha256_hex(bytes: &[u8]) -> Result<String> {
    let mut child = Command::new("sha256sum").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    child.stdin.take().expect("stdin piped").write_all(bytes)?;
    let output = child.wait_with_output()?;
    let out = String::from_utf8_lossy(&output.stdout);
    out.split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("sortie de sha256sum inattendue : {out}"))
}

/// Pousse un exécutable compilé (`bytes`) vers `remote_path` sur `host`. Contrairement à
/// `push_file` (pensé pour de petits fichiers texte), envoie par blocs successifs
/// (`BINARY_CHUNK_SIZE`) plutôt qu'en une seule commande — voir sa doc pour la contrainte
/// Linux exacte que ça contourne. Vérifie l'intégrité par `sha256sum` une fois le transfert
/// terminé (évite d'ajouter une dépendance `sha2` pour un seul usage de vérification,
/// même principe que `local_timestamp()` s'appuyant sur `date`).
pub(crate) fn push_binary(host: &Host, remote_path: &str, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = Path::new(remote_path).parent() {
        let parent = parent.display();
        ssh::exec_shell_checked(host, &format!("mkdir -p '{parent}'"), "création du dossier parent")?;
    }

    if host.hostname == "localhost" {
        std::fs::write(remote_path, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(remote_path, std::fs::Permissions::from_mode(0o755))?;
        }
        return Ok(());
    }

    // Purge un envoi précédent éventuellement interrompu : les blocs suivants
    // s'ajoutent (`>>`), un fichier partiel laissé en place corromprait le résultat.
    ssh::exec_shell_checked(host, &format!("rm -f '{remote_path}'"), "nettoyage d'un envoi précédent")?;

    let total_chunks = bytes.len().div_ceil(BINARY_CHUNK_SIZE).max(1);
    for (i, chunk) in bytes.chunks(BINARY_CHUNK_SIZE).enumerate() {
        let encoded = BASE64.encode(chunk);
        let cmd = format!("echo '{encoded}' | base64 -d >> '{remote_path}'");
        ssh::exec_shell_checked(host, &cmd, &format!("envoi du binaire (bloc {}/{total_chunks})", i + 1))?;
    }

    let local_hash = sha256_hex(bytes)?;
    let remote_check = ssh::exec_shell(host, &format!("sha256sum '{remote_path}'"))?;
    let remote_out = String::from_utf8_lossy(&remote_check.stdout);
    let remote_hash = remote_out.split_whitespace().next().unwrap_or("");
    if remote_hash != local_hash {
        bail!(
            "Intégrité du transfert échouée pour '{remote_path}' : attendu {local_hash}, obtenu '{remote_hash}'"
        );
    }

    ssh::exec_shell_checked(host, &format!("chmod +x '{remote_path}'"), "chmod +x du binaire")?;
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

#[cfg(test)]
mod tests;
