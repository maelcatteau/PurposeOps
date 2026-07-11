//! SSH ControlMaster (port de `ssh-manager.nu`).
//!
//! `is_master_active`/`create_master_connection`/`run_with_master` sont des fonctions
//! internes (comme côté nu : aucune commande CLI publique n'appelle directement le SSH
//! brut, seuls le Docker (Phase 4) et le backup (Phase 6) le feront). Les sockets vivent
//! dans le même dossier `controlmasters/` que le module nu, avec le même nommage
//! `user@hostname:port` — ce qui permet aux deux outils de **réutiliser la même
//! connexion multiplexée** pendant la coexistence.

// Temporaire : is_master_active/create_master_connection/run_with_master ne seront
// appelées par du code CLI qu'en Phase 4 (Docker) et 6 (backup) ; ici elles sont déjà
// couvertes par le test live #[ignore]. À retirer quand Docker les consomme.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, bail};

use crate::config::Host;

fn control_path() -> PathBuf {
    let home = std::env::var("HOME").expect("$HOME non défini");
    let dir = PathBuf::from(home).join("dev/nu-modules/PurposeOps/controlmasters");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).expect("création de controlmasters/");
    }
    dir
}

fn control_socket(host: &Host) -> PathBuf {
    control_path().join(format!("{}@{}:{}", host.user, host.hostname, host.port))
}

fn ssh_target(host: &Host) -> String {
    format!("{}@{}", host.user, host.hostname)
}

/// `~/...` → `$HOME/...` ; `./...` → chemin absolu. Sinon inchangé.
pub fn resolve_key_path(identity_file: &str) -> String {
    if let Some(rest) = identity_file.strip_prefix("~/") {
        let home = std::env::var("HOME").expect("$HOME non défini");
        format!("{home}/{rest}")
    } else if identity_file.starts_with("./") {
        std::fs::canonicalize(identity_file)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| identity_file.to_string())
    } else {
        identity_file.to_string()
    }
}

/// Options communes `-S socket -p port -o ... [-i key]`, sans la cible.
fn common_args(socket: &std::path::Path, host: &Host) -> Vec<String> {
    let mut args = vec![
        "-S".to_string(),
        socket.display().to_string(),
        "-p".to_string(),
        host.port.clone(),
        "-o".to_string(),
        "StrictHostKeyChecking=no".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    if !host.identity_file.is_empty() {
        args.push("-i".to_string());
        args.push(resolve_key_path(&host.identity_file));
    }
    args
}

/// Le socket existe-t-il et la connexion est-elle réellement active (`ssh -O check`) ?
pub fn is_master_active(host: &Host) -> bool {
    let socket = control_socket(host);
    if !socket.exists() {
        return false;
    }
    Command::new("ssh")
        .args(["-O", "check", "-S"])
        .arg(&socket)
        .arg(ssh_target(host))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Crée la connexion maître (`ssh -M -N -f -n ...`). Nettoie un socket orphelin
/// avant de (re)tenter. Une erreur de `ssh -M` peut être bénigne (le vrai verdict
/// est `is_master_active` juste après), donc on ne remonte pas l'erreur ici.
pub fn create_master_connection(host: &Host) -> bool {
    let socket = control_socket(host);
    let target = ssh_target(host);
    println!("🔄 Creating master connection to {target}...");

    if socket.exists() && !is_master_active(host) {
        println!("🧹 Nettoyage d'un socket orphelin...");
        let _ = std::fs::remove_file(&socket);
    }

    let mut args = vec!["-M".to_string(), "-N".to_string(), "-f".to_string(), "-n".to_string()];
    args.extend(common_args(&socket, host));
    args.push(target.clone());

    if let Err(e) = Command::new("ssh").args(&args).status() {
        println!("⚠️ ssh -M a retourné une erreur (potentiellement bénigne) : {e}");
    }

    sleep(Duration::from_millis(500));

    if is_master_active(host) {
        println!("✅ Master connection established.");
        true
    } else {
        println!("❌ Échec création master : le socket existe mais est inactif.");
        if socket.exists() {
            let _ = std::fs::remove_file(&socket);
        }
        false
    }
}

/// Exécute `command` sur l'hôte distant via la connexion maître (créée si besoin).
/// `command` est passé comme un seul argument à `ssh`, qui le transmet tel quel au
/// shell distant — pas de découpage/quoting local, contrairement à un `sh -c` construit
/// par interpolation.
pub fn run_with_master(host: &Host, command: &str) -> Result<Output> {
    if !is_master_active(host) && !create_master_connection(host) {
        bail!("Failed to establish master connection");
    }

    let socket = control_socket(host);
    // Parité avec le nu : échappement des accolades doubles avant transmission.
    let escaped = command.replace("{{", "\\{\\{").replace("}}", "\\}\\}");

    let mut args = common_args(&socket, host);
    args.push(ssh_target(host));
    args.push(escaped);

    Ok(Command::new("ssh").args(&args).output()?)
}

/// Ferme la connexion maître d'un hôte donné. `true` si fermée ou déjà absente.
pub fn close_master_connection(host: &Host) -> bool {
    let socket = control_socket(host);
    let target = ssh_target(host);

    if !socket.exists() {
        println!("ℹ️  No master connection exists for {target}");
        return true;
    }
    if !is_master_active(host) {
        println!("ℹ️  Master connection for {target} is already inactive");
        let _ = std::fs::remove_file(&socket);
        return true;
    }

    println!("🔄 Closing master connection to {target}...");
    let result = Command::new("ssh")
        .args(["-O", "exit", "-S"])
        .arg(&socket)
        .arg(&target)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if result {
        println!("✅ Master connection closed for {target}");
    } else {
        println!("❌ Failed to close master connection");
    }
    if socket.exists() {
        let _ = std::fs::remove_file(&socket);
    }
    result
}

/// `user@hostname:port` → (user, hostname, port). `None` si le nom ne matche pas.
fn parse_socket_name(name: &str) -> Option<(String, String, String)> {
    let (user, rest) = name.split_once('@')?;
    let (hostname, port) = rest.rsplit_once(':')?;
    Some((user.to_string(), hostname.to_string(), port.to_string()))
}

/// Reconstruit un `Host` minimal à partir d'un nom de socket (pas d'identity_file :
/// on n'en a pas besoin pour `-O check`/`-O exit`, qui n'authentifient pas).
fn host_from_socket_name(name: &str) -> Option<Host> {
    let (user, hostname, port) = parse_socket_name(name)?;
    Some(Host {
        name: hostname.clone(),
        hostname,
        user,
        port,
        identity_file: String::new(),
        arch: String::new(),
        docker_context: String::new(),
        identity_key: None,
    })
}

fn list_sockets() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(control_path()) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

/// `closeall` — ferme toutes les connexions maîtres actives.
pub fn close_all_master_connections() {
    println!("🔄 Closing all master connections...");
    let sockets = list_sockets();
    if sockets.is_empty() {
        println!("ℹ️  No master connections found");
        return;
    }

    let mut closed = 0;
    for name in &sockets {
        println!("🔄 Processing {name}...");
        let Some(host) = host_from_socket_name(name) else {
            println!("  ⚠️  Failed to parse {name}");
            continue;
        };
        let target = ssh_target(&host);
        let socket = control_socket(&host);
        let ok = Command::new("ssh")
            .args(["-O", "exit", "-S"])
            .arg(&socket)
            .arg(&target)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            println!("  ✅ Closed connection to {target}");
            closed += 1;
        } else {
            println!("  ⚠️  Failed to close {name}");
        }
        let _ = std::fs::remove_file(&socket);
    }
    println!("✅ Closed {closed} master connections");
}

/// `lsconn` — liste les connexions maîtres avec leur statut.
pub fn list_master_connections() {
    println!("🔍 Active master connections:");
    let sockets = list_sockets();
    if sockets.is_empty() {
        println!("ℹ️  No master connections found");
        return;
    }
    for name in &sockets {
        match host_from_socket_name(name) {
            Some(host) => {
                let status = if is_master_active(&host) {
                    "🟢 ACTIVE"
                } else {
                    "🔴 INACTIVE"
                };
                println!("  {name} - {status}");
            }
            None => println!("  {name} - ❓ UNKNOWN FORMAT"),
        }
    }
}

/// `close` — ferme la connexion de l'hôte actuellement sélectionné dans le contexte.
pub fn close_current_master_connection() -> Result<()> {
    let ctx = crate::config::load_context()?;
    let Some((_, host)) = ctx.host.into_iter().next() else {
        println!("ℹ️  Aucun hôte sélectionné");
        return Ok(());
    };
    if host.hostname == "localhost" {
        println!("ℹ️  No master connection to close for localhost");
        return Ok(());
    }
    close_master_connection(&host);
    Ok(())
}

#[cfg(test)]
mod tests;
