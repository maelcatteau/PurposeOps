//! `bootstrap` — installation des logiciels de base sur un hôte (Phase 10, voir
//! PORTING.md). Capacité entièrement nouvelle, aucun équivalent côté nu.
//!
//! Un ensemble fixe de capacités (Docker, Nushell, Caddy, Netdata) plutôt qu'un fichier
//! de config type `services.yaml` : contrairement aux templates de déploiement, cette
//! liste est définie par l'opérateur, pas par un wizard d'édition à chaud — un module à
//! part le garde séparé du reste de la CRUD config. Chaque capacité a une commande de
//! détection (idempotence) et une commande d'installation, écrites pour un hôte
//! Debian/Ubuntu (apt) : c'est ce que sert la flotte actuelle, pas la peine de détecter
//! la distribution. L'état "installé" n'est jamais mis en cache dans hosts.yaml — chaque
//! run revérifie en direct, ce qui est bon marché à cette échelle et évite tout risque de
//! dérive entre le config et la réalité de l'hôte.

use anyhow::{Result, bail};

use crate::config::{self, Host};
use crate::{ssh, ui};

struct Capability {
    label: &'static str,
    detect: &'static str,
    install: &'static str,
}

const DOCKER_INSTALL: &str = "curl -fsSL https://get.docker.com | sh";

const NUSHELL_INSTALL: &str = r#"set -e
NU_VERSION=$(curl -fsSL https://api.github.com/repos/nushell/nushell/releases/latest | grep -m1 '"tag_name"' | cut -d '"' -f4)
ARCH=$(uname -m)
TMP=$(mktemp -d)
curl -fsSL "https://github.com/nushell/nushell/releases/download/${NU_VERSION}/nu-${NU_VERSION#v}-${ARCH}-unknown-linux-gnu.tar.gz" -o "$TMP/nu.tar.gz"
tar -xzf "$TMP/nu.tar.gz" -C "$TMP"
sudo install -m 0755 "$TMP"/nu-*/nu /usr/local/bin/nu
rm -rf "$TMP""#;

const CADDY_INSTALL: &str = r#"set -e
sudo apt-get update
sudo apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl gnupg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --yes --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt-get update
sudo apt-get install -y caddy"#;

const NETDATA_INSTALL: &str = "curl -fsSL https://get.netdata.cloud/kickstart.sh | sh -s -- --non-interactive";

/// Installé directement sur l'hôte (pas en conteneur) : Netdata découvre alors tout seul
/// les conteneurs Docker locaux, et n'a pas besoin d'être piloté via `provision`/services.yaml.
const CAPABILITIES: &[Capability] = &[
    Capability { label: "Docker", detect: "command -v docker", install: DOCKER_INSTALL },
    Capability { label: "Nushell", detect: "command -v nu", install: NUSHELL_INSTALL },
    Capability { label: "Caddy", detect: "command -v caddy", install: CADDY_INSTALL },
    Capability { label: "Netdata", detect: "command -v netdata", install: NETDATA_INSTALL },
];

/// Capacités absentes selon `present` (injecté pour rester testable sans SSH réel).
fn missing_capabilities(present: impl Fn(&Capability) -> bool) -> Vec<&'static Capability> {
    CAPABILITIES.iter().filter(|c| !present(c)).collect()
}

fn is_installed(host: &Host, cap: &Capability) -> bool {
    ssh::exec_shell(host, cap.detect).map(|o| o.status.success()).unwrap_or(false)
}

/// `bootstrap` — arg direct ou menu fuzzy pour l'hôte, détection en direct des capacités
/// déjà présentes, sélection multiple de ce qui manque, confirmation unique, installation.
pub fn cmd_bootstrap(host_id: Option<String>) -> Result<()> {
    let hosts = config::load_hosts()?;
    let id = match host_id {
        Some(id) => id,
        None => match ui::select("Host à bootstrap :", hosts.keys().cloned().collect()) {
            Some(id) => id,
            None => return Ok(()),
        },
    };
    let Some(host) = hosts.get(&id) else {
        bail!("Host '{id}' introuvable");
    };

    println!("🔍 Vérification des logiciels déjà présents sur '{id}'...");
    for cap in CAPABILITIES {
        let present = is_installed(host, cap);
        println!("  {} {}", if present { "✅" } else { "⬜" }, cap.label);
    }

    let missing = missing_capabilities(|c| is_installed(host, c));
    if missing.is_empty() {
        println!("Tout est déjà installé.");
        return Ok(());
    }

    let labels: Vec<String> = missing.iter().map(|c| c.label.to_string()).collect();
    let Some(chosen) = ui::multi_select("Que faut-il installer ?", labels) else {
        return Ok(());
    };
    if chosen.is_empty() {
        println!("❌ Rien de sélectionné");
        return Ok(());
    }

    if !ui::confirm(&format!("Installer {} sur '{id}' ?", chosen.join(", "))) {
        println!("❌ Opération annulée");
        return Ok(());
    }

    for cap in missing.into_iter().filter(|c| chosen.contains(&c.label.to_string())) {
        println!("📦 Installation de {}...", cap.label);
        ssh::exec_shell_checked(host, cap.install, cap.label)?;
        println!("✅ {} installé", cap.label);
    }

    Ok(())
}

#[cfg(test)]
mod tests;
