//! Commandes Docker (port de `docker/`).
//!
//! Différence volontaire avec le nu : la liste de conteneurs/réseaux se parse via
//! `docker ... --format json` (NDJSON, une ligne = un objet `serde_json`) plutôt que le
//! parsing `from ssv -a` de colonnes alignées à espaces — plus robuste, docker le
//! supporte nativement. Le filtre `=~` du nu (regex) est reproduit avec le crate `regex`.

use std::process::Output;

use anyhow::{Result, anyhow, bail};
use regex::Regex;
use serde::Deserialize;

use crate::config::Host;
use crate::{host, ssh, table, ui};

/// Quote un argument pour un shell POSIX distant (même logique que `shell-quote` en nu :
/// chaque argument devient un mot unique même s'il contient des espaces).
fn shell_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', "'\\''"))
}

/// Exécute `docker <command...>` en local, ou via SSH ControlMaster si l'hôte est distant.
/// Le branchement local passe par `ssh::spawn` (pas un `Command::new("docker")` direct)
/// pour réutiliser le même seam de test que `ssh.rs` plutôt que d'en dupliquer un.
pub fn run_docker_command(command: &[&str], host: &Host) -> Result<Output> {
    if host.hostname == "localhost" {
        let args: Vec<String> = command.iter().map(|a| a.to_string()).collect();
        Ok(ssh::spawn("docker", &args)?)
    } else {
        let mut parts = vec!["docker".to_string()];
        parts.extend(command.iter().map(|a| shell_quote(a)));
        Ok(ssh::run_with_master(host, &parts.join(" "))?)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PsEntry {
    names: String,
    image: String,
    status: String,
    ports: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NetworkEntry {
    name: String,
    driver: String,
    scope: String,
}

fn parse_ndjson<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<Vec<T>> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| Ok(serde_json::from_str(l)?))
        .collect()
}

fn get_containers(need_all: bool, host: &Host) -> Result<Vec<PsEntry>> {
    let cmd: &[&str] = if need_all {
        &["ps", "-a", "--format", "json"]
    } else {
        &["ps", "--format", "json"]
    };
    let output = run_docker_command(cmd, host)?;
    parse_ndjson(&output.stdout)
}

fn get_networks(host: &Host) -> Result<Vec<NetworkEntry>> {
    let output = run_docker_command(&["network", "ls", "--format", "json"], host)?;
    parse_ndjson(&output.stdout)
}

/// `docker inspect <container> | .[0].NetworkSettings.Networks`.
fn extract_networks(container: &str, host: &Host) -> Result<serde_json::Value> {
    let output = run_docker_command(&["inspect", container], host)?;
    if !output.status.success() {
        bail!("docker inspect a échoué pour '{container}'");
    }
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    parsed
        .get(0)
        .and_then(|c| c.get("NetworkSettings"))
        .and_then(|n| n.get("Networks"))
        .cloned()
        .ok_or_else(|| anyhow!("NetworkSettings.Networks introuvable dans docker inspect"))
}

/// Hôte courant (équivalent de `with_host_info` en nu, sans la resynthèse partielle :
/// le record `localhost` de hosts.yaml est déjà complet).
fn current_host() -> Result<Host> {
    host::get_current_host()?
        .map(|(_, h)| h)
        .ok_or_else(|| anyhow!("Aucun hôte sélectionné dans le contexte."))
}

fn regex_contains(filter: &Option<String>, value: &str) -> Result<bool> {
    match filter {
        None => Ok(true),
        Some(f) => Ok(Regex::new(f)?.is_match(value)),
    }
}

/// Sélectionne un conteneur (menu fuzzy) parmi ceux disponibles. `None` si liste
/// vide (message déjà affiché) ou annulation.
fn select_container(need_all: bool, header: &str, host: &Host) -> Result<Option<String>> {
    let entries = get_containers(need_all, host)?;
    if entries.is_empty() {
        println!("No containers available");
        return Ok(None);
    }
    let names: Vec<String> = entries.into_iter().map(|e| e.names).collect();
    Ok(ui::select(header, names))
}

#[derive(Clone, Copy)]
enum ContainerOp {
    Start,
    Stop,
    Restart,
}

impl ContainerOp {
    fn need_all(self) -> bool {
        matches!(self, ContainerOp::Start)
    }
    fn header(self) -> &'static str {
        match self {
            ContainerOp::Start => "Select a container to start:",
            ContainerOp::Stop => "Select a container to stop:",
            ContainerOp::Restart => "Select a container to restart:",
        }
    }
    fn verb(self) -> &'static str {
        match self {
            ContainerOp::Start => "Starting",
            ContainerOp::Stop => "Stopping",
            ContainerOp::Restart => "Restarting",
        }
    }
    fn past_participle(self) -> &'static str {
        match self {
            ContainerOp::Start => "started",
            ContainerOp::Stop => "stopped",
            ContainerOp::Restart => "restarted",
        }
    }
    fn docker_command(self) -> &'static str {
        match self {
            ContainerOp::Start => "start",
            ContainerOp::Stop => "stop",
            ContainerOp::Restart => "restart",
        }
    }
}

fn run_simple_op(op: ContainerOp) -> Result<()> {
    let host = current_host()?;
    let Some(container) = select_container(op.need_all(), op.header(), &host)? else {
        println!("Operation cancelled - no container selected");
        return Ok(());
    };
    println!("{} container: {container}", op.verb());
    let output = run_docker_command(&[op.docker_command(), &container], &host)?;
    if output.status.success() {
        println!("✅ Container {container} {} successfully", op.past_participle());
    } else {
        println!("❌ Failed to {} container {container}", op.docker_command());
    }
    Ok(())
}

pub fn cmd_start() -> Result<()> {
    run_simple_op(ContainerOp::Start)
}
pub fn cmd_stop() -> Result<()> {
    run_simple_op(ContainerOp::Stop)
}
pub fn cmd_restart() -> Result<()> {
    run_simple_op(ContainerOp::Restart)
}

/// `dn extract` — extrait et affiche les réseaux d'un conteneur choisi (JSON indenté).
pub fn cmd_dn_extract() -> Result<()> {
    let host = current_host()?;
    let Some(container) = select_container(true, "Select a container to extract networks from:", &host)? else {
        println!("Operation cancelled - no container selected");
        return Ok(());
    };
    println!("Extracting networks from container: {container}");
    match extract_networks(&container, &host) {
        Ok(networks) => {
            println!("✅ Container {container} networks extracted from successfully");
            println!("{}", serde_json::to_string_pretty(&networks)?);
        }
        Err(_) => println!("❌ Failed to networks_extract container {container}"),
    }
    Ok(())
}

/// `dps` — statut des conteneurs en cours (optionnellement filtré par regex sur le nom).
pub fn cmd_dps(filter: Option<String>, ports: bool) -> Result<()> {
    let host = current_host()?;
    let entries = get_containers(false, &host)?;
    let mut rows = Vec::new();
    for e in &entries {
        if !regex_contains(&filter, &e.names)? {
            continue;
        }
        if ports {
            rows.push(vec![
                e.names.clone(),
                e.image.clone(),
                e.status.clone(),
                e.ports.clone(),
            ]);
        } else {
            rows.push(vec![e.names.clone(), e.image.clone(), e.status.clone()]);
        }
    }
    if ports {
        table::print(&["NAMES", "IMAGE", "STATUS", "PORTS"], &rows);
    } else {
        table::print(&["NAMES", "IMAGE", "STATUS"], &rows);
    }
    Ok(())
}

/// `dnls` — liste des réseaux Docker (optionnellement filtrée par regex sur le nom).
pub fn cmd_dnls(filter: Option<String>) -> Result<()> {
    let host = current_host()?;
    let mut rows = Vec::new();
    for n in get_networks(&host)? {
        if regex_contains(&filter, &n.name)? {
            rows.push(vec![n.name.clone(), n.driver.clone(), n.scope.clone()]);
        }
    }
    table::print(&["NAME", "DRIVER", "SCOPE"], &rows);
    Ok(())
}

#[cfg(test)]
mod tests;
