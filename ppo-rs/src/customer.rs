//! Commandes client (port de `customer-manager/`).

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};

use crate::config::{self, Customer, CustomerLite};
use crate::{host, ui};

/// (name, CustomerLite) courant, ou `None`.
pub fn get_current_customer() -> Result<Option<(String, CustomerLite)>> {
    Ok(config::load_context()?.customer.into_iter().next())
}

/// `c` — client courant (nom + abréviation).
pub fn cmd_c() -> Result<()> {
    match get_current_customer()? {
        Some((name, c)) => println!("{name} ({})", c.abbreviation),
        None => println!("No customer currently selected"),
    }
    Ok(())
}

/// `cname` — nom du client courant seulement.
pub fn cmd_cname() -> Result<()> {
    match get_current_customer()? {
        Some((name, _)) => println!("{name}"),
        None => println!("No customer currently selected"),
    }
    Ok(())
}

/// `lsc` — liste des clients : nom, abréviation, nb de déploiements, marqueur courant.
pub fn cmd_lsc() -> Result<()> {
    let customers = config::load_customers()?;
    let current = get_current_customer()?.map(|(n, _)| n);
    for (name, c) in &customers {
        let marker = if current.as_deref() == Some(name.as_str()) {
            " *"
        } else {
            ""
        };
        println!(
            "{name}\t{}\t{} deployment(s){marker}",
            c.abbreviation,
            c.deployments.len()
        );
    }
    Ok(())
}

/// Écrit `context.customer = {name: <client moins deployments/hosts>}`.
fn set_customer_internal(name: &str, cust: &Customer) -> Result<()> {
    let lite = CustomerLite {
        abbreviation: cust.abbreviation.clone(),
    };
    let mut ctx = config::load_context()?;
    ctx.customer = BTreeMap::from([(name.to_string(), lite)]);
    config::save_context(&ctx)?;
    println!("📍 Context set to: {name}");
    Ok(())
}

/// `sc` — arg direct (sans vérif d'hôte, comme le nu) ou menu fuzzy (avec vérif de
/// cohérence hôte↔client, proposant de basculer l'hôte si besoin).
pub fn cmd_sc(customer: Option<String>) -> Result<()> {
    let customers = config::load_customers()?;

    let name = match customer {
        Some(n) => {
            let cust = customers
                .get(&n)
                .ok_or_else(|| anyhow!("Customer '{n}' introuvable"))?;
            return set_customer_internal(&n, cust);
        }
        None => match ui::select("Select customer :", customers.keys().cloned().collect()) {
            Some(n) => n,
            None => return Ok(()),
        },
    };

    let cust = customers
        .get(&name)
        .ok_or_else(|| anyhow!("Customer '{name}' introuvable"))?;

    // Cohérence hôte↔client : l'hôte courant est-il un hôte de ce client ?
    let customer_host_ids: Vec<&str> = cust.hosts.iter().map(|h| h.host_id.as_str()).collect();
    let current_host = host::get_current_host()?.map(|(id, _)| id);
    let consistent = current_host
        .as_deref()
        .is_some_and(|h| customer_host_ids.contains(&h));

    if consistent {
        set_customer_internal(&name, cust)
    } else if let Some(target) = customer_host_ids.first() {
        let cur = current_host.as_deref().unwrap_or("(aucun)");
        println!("L'hôte actuel '{cur}' n'est pas un hôte du client '{name}'.");
        if ui::confirm(&format!("Basculer aussi sur l'hôte '{target}' ?")) {
            host::set_host(target)?;
            set_customer_internal(&name, cust)
        } else {
            println!("⚠️ Changement de client annulé. L'hôte reste inchangé.");
            Ok(())
        }
    } else {
        // Client sans hôte défini : on applique quand même.
        set_customer_internal(&name, cust)
    }
}
