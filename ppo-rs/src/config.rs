//! Couche config : chemins + modèles serde des YAML de PurposeOps-config.
//!
//! Pour l'instant (Phase 1) seul le contexte est modélisé. Les phases suivantes
//! ajouteront ici les modèles de `hosts.yaml`, `customers.yaml`, `services.yaml`.
//!
//! Les chemins sont codés en dur comme côté nu (`config/config.nu`) pour garantir
//! que les deux outils lisent/écrivent EXACTEMENT le même fichier pendant la coexistence.

// Temporaire : plusieurs structs/champs ne seront câblés qu'en Phase 2 (couche config
// complète). À retirer quand tout est utilisé.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

/// Racine du repo, dérivée de $HOME comme le fait `config.nu` (`~/dev/nu-modules/PurposeOps`).
fn base_path() -> PathBuf {
    let home = std::env::var("HOME").expect("$HOME non défini");
    PathBuf::from(home).join("dev/nu-modules/PurposeOps")
}

pub fn context_path() -> PathBuf {
    base_path().join("PurposeOps-config/context.yaml")
}

/// Un hôte (localhost ou VPS). `port` et `identity_file` restent des String :
/// le YAML contient `port: ''` pour localhost et `'2222'` pour les VPS — les typer
/// en entier casserait le round-trip côté nu tant que la coexistence dure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub name: String,
    pub hostname: String,
    pub user: String,
    pub port: String,
    pub identity_file: String,
    pub arch: String,
    pub docker_context: String,
}

/// Dans le contexte, le client n'est stocké que par son abréviation
/// (le record complet vit dans customers.yaml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerLite {
    pub abbreviation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentHost {
    pub host_id: String,
    pub path_for_service: String,
    pub path_for_docker_compose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbCredentials {
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
}

/// Un déploiement. Les champs DB sont `Option` car absents pour les services
/// sans base (Vaultwarden, Caddy). `skip_serializing_if` évite d'écrire des
/// `champ: null` inutiles pour ces services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub service_name: String,
    pub hosts: Vec<DeploymentHost>,
    pub deployment_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_container_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_credentials: Option<DbCredentials>,
}

/// Le champ `deployment` du contexte peut être :
/// - `null` → `None` (géré par l'`Option` dans `Context`),
/// - un record complet → `Full`,
/// - une ancienne string d'id (avant migration) → `Legacy`.
///
/// `untagged` : serde essaie les variantes dans l'ordre. Une map matche `Full`,
/// une string ne matche pas `Full` (attend une map) puis matche `Legacy`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DeploymentField {
    Full(Box<Deployment>),
    Legacy(String),
}

/// L'état « sélection courante » de la session (équivalent du kubectl current-context).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// Une seule entrée : l'hôte courant (clé = host_id).
    pub host: BTreeMap<String, Host>,
    pub prompt_show: bool,
    /// Vide `{}` si aucun client sélectionné.
    #[serde(default)]
    pub customer: BTreeMap<String, CustomerLite>,
    #[serde(default)]
    pub deployment: Option<DeploymentField>,
}

pub fn load_context() -> Result<Context> {
    let path = context_path();
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("lecture de {}", path.display()))?;
    let ctx = serde_yaml_ng::from_str(&text)
        .with_context(|| format!("parsing YAML de {}", path.display()))?;
    Ok(ctx)
}

pub fn save_context(ctx: &Context) -> Result<()> {
    let path = context_path();
    let text = serde_yaml_ng::to_string(ctx)?;
    std::fs::write(&path, text).with_context(|| format!("écriture de {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests;
