//! Commandes déploiement (port de `deployment-manager/`).

use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};

use crate::config::{self, Customer, Deployment, DeploymentField};
use crate::{customer, host, ui};

/// Record du déploiement courant. Erreurs explicites : absent, ou ancien format string.
pub fn get_current_deployment_info() -> Result<Deployment> {
    match config::load_context()?.deployment {
        None => bail!("Aucun déploiement sélectionné dans le contexte."),
        Some(DeploymentField::Legacy(_)) => bail!(
            "Format de contexte obsolète (ID simple). Re-sélectionnez le déploiement avec 'ppor sd'."
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
    for d in &deps {
        let host = d.hosts.first().map(|h| h.host_id.as_str()).unwrap_or("?");
        println!("{}\t{}\t{host}", d.deployment_id, d.service_name);
    }
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
        anyhow!("Aucun client sélectionné. Utilisez 'ppor sc <client>' d'abord.")
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
