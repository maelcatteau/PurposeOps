//! `ppor check` — validation de cohérence de toute la config (capacité nouvelle,
//! inexistante côté nu). Utile avant un merge et pour détecter les configs cassées.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, bail};

use crate::config::{self, Context, Customer, DeploymentField, Host};

pub fn cmd_check() -> Result<()> {
    let hosts = config::load_hosts()?;
    let customers = config::load_customers()?;
    let services = config::load_services()?;
    let ctx = config::load_context()?;

    let problems = find_problems(&hosts, &customers, &ctx);

    if problems.is_empty() {
        println!(
            "✅ Config cohérente : {} hôtes, {} clients, {} déploiements, {} services.",
            hosts.len(),
            customers.len(),
            customers.values().map(|c| c.deployments.len()).sum::<usize>(),
            services.len()
        );
        Ok(())
    } else {
        for p in &problems {
            eprintln!("❌ {p}");
        }
        bail!("{} incohérence(s) détectée(s)", problems.len());
    }
}

/// Cœur pur et testable : renvoie la liste des incohérences (vide = tout va bien).
pub fn find_problems(
    hosts: &BTreeMap<String, Host>,
    customers: &BTreeMap<String, Customer>,
    ctx: &Context,
) -> Vec<String> {
    let mut problems: Vec<String> = Vec::new();

    // deployment_id → clients qui le portent (pour détecter les doublons globaux).
    let mut dep_owners: HashMap<String, Vec<String>> = HashMap::new();

    for (cname, cust) in customers {
        // Hôtes référencés par le client.
        for ch in &cust.hosts {
            if !hosts.contains_key(&ch.host_id) {
                problems.push(format!(
                    "Client '{cname}' référence l'hôte inconnu '{}'",
                    ch.host_id
                ));
            }
        }
        // Hôtes référencés par chaque déploiement + collecte des ids.
        for dep in &cust.deployments {
            for dh in &dep.hosts {
                if !hosts.contains_key(&dh.host_id) {
                    problems.push(format!(
                        "Déploiement '{}' (client '{cname}') référence l'hôte inconnu '{}'",
                        dep.deployment_id, dh.host_id
                    ));
                }
            }
            dep_owners
                .entry(dep.deployment_id.clone())
                .or_default()
                .push(cname.clone());
        }
    }

    for (id, owners) in &dep_owners {
        if owners.len() > 1 {
            problems.push(format!("deployment_id '{id}' dupliqué (clients : {owners:?})"));
        }
    }

    // Le contexte pointe-t-il vers des entrées valides ?
    if let Some((hid, _)) = ctx.host.iter().next()
        && !hosts.contains_key(hid)
    {
        problems.push(format!("Contexte : hôte courant '{hid}' absent de hosts.yaml"));
    }
    if let Some((cname, _)) = ctx.customer.iter().next()
        && !customers.contains_key(cname)
    {
        problems.push(format!(
            "Contexte : client courant '{cname}' absent de customers.yaml"
        ));
    }
    if let Some(DeploymentField::Legacy(id)) = &ctx.deployment {
        problems.push(format!(
            "Contexte : déploiement en ancien format string ('{id}') — re-sélectionner avec 'ppor sd'"
        ));
    }

    problems
}

#[cfg(test)]
mod tests;
