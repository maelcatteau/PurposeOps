//! Commandes hôte (port de `machine-manager/`).

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::config::{self, Host};
use crate::{table, ui};

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
    let rows: Vec<Vec<String>> = hosts
        .iter()
        .map(|(id, host)| {
            let kind = if host.hostname == "localhost" {
                "local"
            } else {
                "remote"
            };
            let marker = if current.as_deref() == Some(id.as_str()) {
                "*"
            } else {
                ""
            };
            vec![id.clone(), host.name.clone(), kind.to_string(), marker.to_string()]
        })
        .collect();
    table::print(&["HOST", "NAME", "TYPE", "CURRENT"], &rows);
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

/// `ch` (create_host) — wizard interactif, aperçu YAML, confirmation, insertion dans
/// hosts.yaml. Port de `hosts-config-manager.nu`'s `create_host`.
pub fn cmd_ch() -> Result<()> {
    let Some(host_name) = ui::text("Enter the new host_name : ") else {
        return Ok(());
    };

    let mut hosts = config::load_hosts()?;
    if hosts.contains_key(&host_name) {
        println!("❌ Host '{host_name}' already exists");
        return Ok(());
    }

    let Some(hostname) = ui::text("Enter the new hostname (ip) : ") else {
        return Ok(());
    };
    let Some(user) = ui::text("Enter the user for the new host : ") else {
        return Ok(());
    };
    let Some(port) = ui::text("Enter the port for the new host : ") else {
        return Ok(());
    };
    if port.trim().parse::<u32>().is_err() {
        println!("❌ Port must be a valid number");
        return Ok(());
    }
    let Some(identity_file) = ui::text("Enter the path for the ssh id file for the new host : ")
    else {
        return Ok(());
    };
    let Some(arch) = ui::text("Enter the correct architecture ('x86_64', 'arm64') : ") else {
        return Ok(());
    };

    let new_host = Host {
        name: format!("vps-{host_name}"),
        hostname,
        user,
        port,
        identity_file,
        arch,
        docker_context: format!("remote-{host_name}"),
        identity_key: None,
    };

    println!(
        "Voulez vous valider ce nouvel hote ? {}",
        serde_yaml_ng::to_string(&new_host)?
    );
    if !ui::confirm("Valider ?") {
        println!("Opération annulée");
        return Ok(());
    }

    hosts.insert(host_name, new_host);
    config::save_yaml_map(&config::hosts_config_path(), &hosts)
}

/// `dh` (delete host) — sélection fuzzy + confirmation + suppression.
pub fn cmd_dh() -> Result<()> {
    config::delete_from_map::<Host>(config::hosts_config_path(), "host")
}
