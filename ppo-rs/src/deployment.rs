//! Commandes déploiement (port de `deployment-manager/`).

use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};

use crate::config::{self, Customer, DbCredentials, Deployment, DeploymentField, DeploymentHost};
use crate::{customer, host, secrets, table, ui};

/// Record du déploiement courant. Erreurs explicites : absent, ou ancien format string.
pub fn get_current_deployment_info() -> Result<Deployment> {
    match config::load_context()?.deployment {
        None => bail!("Aucun déploiement sélectionné dans le contexte."),
        Some(DeploymentField::Legacy(_)) => bail!(
            "Format de contexte obsolète (ID simple). Re-sélectionnez le déploiement avec 'ppo sd'."
        ),
        Some(DeploymentField::Full(d)) => Ok(*d),
    }
}

/// `pde` — id du déploiement courant.
pub fn cmd_pde() -> Result<()> {
    println!("{}", get_current_deployment_info()?.deployment_id);
    Ok(())
}

/// `pdei` — record complet du déploiement courant (YAML).
pub fn cmd_pdei() -> Result<()> {
    print!("{}", serde_yaml_ng::to_string(&get_current_deployment_info()?)?);
    Ok(())
}

/// Déploiements du client courant.
pub fn list_deployments_for_current_customer() -> Result<Vec<Deployment>> {
    let (name, _) =
        customer::get_current_customer()?.ok_or_else(|| anyhow!("Aucun client sélectionné."))?;
    let customers = config::load_customers()?;
    let cust = customers
        .get(&name)
        .ok_or_else(|| anyhow!("Client '{name}' introuvable dans customers.yaml"))?;
    Ok(cust.deployments.clone())
}

/// `lsd` — liste : deployment_id, service, hôte.
pub fn cmd_lsd() -> Result<()> {
    let deps = list_deployments_for_current_customer()?;
    if deps.is_empty() {
        println!("(aucun déploiement pour ce client)");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = deps
        .iter()
        .map(|d| {
            let host = d.hosts.first().map(|h| h.host_id.as_str()).unwrap_or("?");
            vec![d.deployment_id.clone(), d.service_name.clone(), host.to_string()]
        })
        .collect();
    table::print(&["DEPLOYMENT_ID", "SERVICE", "HOST"], &rows);
    Ok(())
}

/// Hôte (premier) d'un deployment_id, cherché sur **tous** les clients (ids globaux).
fn host_for_deployment(deployment_id: &str, customers: &BTreeMap<String, Customer>) -> Option<String> {
    for cust in customers.values() {
        for d in &cust.deployments {
            if d.deployment_id == deployment_id {
                return d.hosts.first().map(|h| h.host_id.clone());
            }
        }
    }
    None
}

/// Écrit `context.deployment = <record complet>` (résolu dans le client courant).
fn set_deployment_internal(deployment_id: &str) -> Result<()> {
    let (customer_name, _) = customer::get_current_customer()?
        .ok_or_else(|| anyhow!("Aucun client sélectionné dans le contexte."))?;
    let customers = config::load_customers()?;
    let cust = customers
        .get(&customer_name)
        .ok_or_else(|| anyhow!("Client '{customer_name}' introuvable"))?;
    let dep = cust
        .deployments
        .iter()
        .find(|d| d.deployment_id == deployment_id)
        .ok_or_else(|| {
            anyhow!("Déploiement '{deployment_id}' introuvable pour le client '{customer_name}'.")
        })?
        .clone();

    let mut ctx = config::load_context()?;
    let host_id = dep.hosts.first().map(|h| h.host_id.clone());
    let service_name = dep.service_name.clone();
    ctx.deployment = Some(DeploymentField::Full(Box::new(dep)));
    config::save_context(&ctx)?;

    println!("📍 Déploiement actif : {service_name}");
    println!(" ID : {deployment_id}");
    println!(" sur hôte {}", host_id.as_deref().unwrap_or("?"));
    Ok(())
}

/// `sd` — exige un client sélectionné. Arg direct (sans vérif d'hôte) ou menu fuzzy
/// (avec vérif de cohérence d'hôte comme le nu).
pub fn cmd_sd(deployment_id: Option<String>) -> Result<()> {
    let (customer_name, _) = customer::get_current_customer()?.ok_or_else(|| {
        anyhow!("Aucun client sélectionné. Utilisez 'ppo sc <client>' d'abord.")
    })?;
    let customers = config::load_customers()?;
    let cust = customers
        .get(&customer_name)
        .ok_or_else(|| anyhow!("Client '{customer_name}' introuvable"))?;
    let available: Vec<String> = cust
        .deployments
        .iter()
        .map(|d| d.deployment_id.clone())
        .collect();

    let selected = match deployment_id {
        Some(id) => {
            if !available.contains(&id) {
                bail!("Déploiement '{id}' introuvable pour le client '{customer_name}'");
            }
            return set_deployment_internal(&id);
        }
        None => match ui::select("Sélectionnez un déploiement :", available) {
            Some(s) => s,
            None => return Ok(()),
        },
    };

    // Cohérence d'hôte : le déploiement cible impose-t-il un autre hôte ?
    let current_host = host::get_current_host()?.map(|(id, _)| id);
    let target_host = host_for_deployment(&selected, &customers);

    if current_host == target_host {
        set_deployment_internal(&selected)
    } else if let Some(target) = target_host {
        let cur = current_host.as_deref().unwrap_or("(aucun)");
        println!("⚠️ Le déploiement '{selected}' est sur l'hôte '{target}'.");
        println!("   L'hôte actuel est '{cur}'.");
        if ui::confirm(&format!("Basculer sur l'hôte '{target}' ?")) {
            host::set_host(&target)?;
            set_deployment_internal(&selected)
        } else {
            println!("⚠️ Changement de déploiement annulé. L'hôte reste inchangé.");
            Ok(())
        }
    } else {
        set_deployment_internal(&selected)
    }
}

/// Un `deployment_id` doit être unique **globalement** (tous clients confondus) —
/// `host_for_deployment` et les autres lookups cherchent par id sans préciser de client.
fn deployment_id_exists(deployment_id: &str, customers: &BTreeMap<String, Customer>) -> bool {
    customers
        .values()
        .any(|c| c.deployments.iter().any(|d| d.deployment_id == deployment_id))
}

/// `cdep` (create_deployment) — pour le client courant : wizard interactif, validation
/// host + unicité globale de l'id, champs DB optionnels, aperçu YAML, confirmation,
/// append dans `customers.yaml`. Port de `deployment-manager/core.nu`'s `create_deployment`.
pub fn cmd_cdep() -> Result<()> {
    let (customer_name, _) = customer::get_current_customer()?
        .ok_or_else(|| anyhow!("Aucun client sélectionné. Utilisez 'sc <client>' d'abord."))?;

    let hosts = config::load_hosts()?;
    println!("📍 Création d'un déploiement pour : {customer_name}");

    let Some(service_name) = ui::text("Service name (ex: Odoo CE, Vaultwarden): ") else {
        return Ok(());
    };
    let Some(host_id) = ui::text("Host ID: ") else {
        return Ok(());
    };

    if !hosts.contains_key(&host_id) {
        let available: Vec<&String> = hosts.keys().collect();
        println!("❌ Host '{host_id}' introuvable ! Hôtes disponibles : {available:?}");
        return Ok(());
    }

    let Some(path_for_service) = ui::text("Path for service on host: ") else {
        return Ok(());
    };
    let Some(path_for_docker_compose) = ui::text("Path for docker-compose file: ") else {
        return Ok(());
    };
    let Some(deployment_id) = ui::text("Deployment id (unique): ") else {
        return Ok(());
    };

    let mut customers = config::load_customers()?;
    if deployment_id_exists(&deployment_id, &customers) {
        println!("❌ Le deployment_id '{deployment_id}' est déjà utilisé par un autre déploiement.");
        return Ok(());
    }

    let mut new_deployment = Deployment {
        service_name,
        hosts: vec![DeploymentHost {
            host_id,
            path_for_service,
            path_for_docker_compose,
        }],
        deployment_id: deployment_id.clone(),
        container_name: None,
        db_container_name: None,
        database_name: None,
        db_credentials: None,
    };

    if ui::confirm("Ce déploiement a-t-il une base de données à sauvegarder ?") {
        let Some(container_name) = ui::text("Container name: ") else {
            return Ok(());
        };
        let Some(db_container_name) = ui::text("DB container name: ") else {
            return Ok(());
        };
        let Some(database_name) = ui::text("Database name: ") else {
            return Ok(());
        };
        let Some(db_host) = ui::text("DB credentials - host: ") else {
            return Ok(());
        };
        let Some(db_port) = ui::text("DB credentials - port: ") else {
            return Ok(());
        };
        let Some(db_user) = ui::text("DB credentials - user: ") else {
            return Ok(());
        };
        let Some(db_password) = ui::text("DB credentials - password: ") else {
            return Ok(());
        };

        // Chiffré immédiatement à la clé du client (génère sa clé si absente) : aucun
        // mot de passe en clair n'atteint le disque pour un déploiement créé après la
        // Phase 8. Voir PORTING.md.
        let customer_identity = secrets::load_or_generate_customer_identity(&customer_name)?;
        let encrypted_password =
            secrets::encrypt_secret(&db_password, &[customer_identity.to_public()])?;

        new_deployment.container_name = Some(container_name);
        new_deployment.db_container_name = Some(db_container_name);
        new_deployment.database_name = Some(database_name);
        new_deployment.db_credentials = Some(DbCredentials {
            host: db_host,
            port: db_port,
            user: db_user,
            password: encrypted_password,
        });
    }

    println!("{}", serde_yaml_ng::to_string(&new_deployment)?);
    if !ui::confirm("Créer ce déploiement ?") {
        println!("❌ Opération annulée");
        return Ok(());
    }

    let cust = customers
        .get_mut(&customer_name)
        .ok_or_else(|| anyhow!("Client '{customer_name}' introuvable"))?;
    cust.deployments.push(new_deployment);
    config::save_yaml_map(&config::customers_config_path(), &customers)?;

    println!("✅ Déploiement '{deployment_id}' créé pour '{customer_name}'");
    Ok(())
}
