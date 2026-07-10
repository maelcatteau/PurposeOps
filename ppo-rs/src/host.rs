//! Commandes hôte (port de `machine-manager/`).

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::config::{self, Host};
use crate::ui;

/// (host_id, Host) courant, ou `None` si aucun hôte sélectionné.
pub fn get_current_host() -> Result<Option<(String, Host)>> {
    Ok(config::load_context()?.host.into_iter().next())
}

/// `hname` — nom (id) de l'hôte courant seulement.
pub fn cmd_hname() -> Result<()> {
    match get_current_host()? {
        Some((id, _)) => println!("{id}"),
        None => println!("No host currently selected"),
    }
    Ok(())
}

/// `h` — record complet de l'hôte courant (affiché en YAML, comme le record nu).
pub fn cmd_h() -> Result<()> {
    match get_current_host()? {
        Some((id, host)) => {
            let map = BTreeMap::from([(id, host)]);
            print!("{}", serde_yaml_ng::to_string(&map)?);
        }
        None => println!("No host currently selected"),
    }
    Ok(())
}

/// `lsh` — liste des hôtes : id, nom, type (local/remote), marqueur courant.
pub fn cmd_lsh() -> Result<()> {
    let hosts = config::load_hosts()?;
    let current = get_current_host()?.map(|(id, _)| id);
    for (id, host) in &hosts {
        let kind = if host.hostname == "localhost" {
            "local"
        } else {
            "remote"
        };
        let marker = if current.as_deref() == Some(id.as_str()) {
            " *"
        } else {
            ""
        };
        println!("{id}\t{}\t{kind}{marker}", host.name);
    }
    Ok(())
}

/// Écrit `context.host = {host_id: <record de hosts.yaml>}`. Réutilisé par `sc`/`sd`.
pub fn set_host(host_id: &str) -> Result<()> {
    let hosts = config::load_hosts()?;
    let Some(host) = hosts.get(host_id).cloned() else {
        let available: Vec<_> = hosts.keys().cloned().collect();
        bail!("Host '{host_id}' introuvable. Disponibles : {available:?}");
    };
    let mut ctx = config::load_context()?;
    ctx.host = BTreeMap::from([(host_id.to_string(), host.clone())]);
    config::save_context(&ctx)?;
    println!("📍 Context set to: {}", host.name);
    Ok(())
}

/// `sh` — arg direct ou menu fuzzy.
pub fn cmd_sh(host_id: Option<String>) -> Result<()> {
    let id = match host_id {
        Some(id) => id,
        None => {
            let hosts = config::load_hosts()?;
            match ui::select("Select host :", hosts.keys().cloned().collect()) {
                Some(id) => id,
                None => return Ok(()),
            }
        }
    };
    set_host(&id)
}
