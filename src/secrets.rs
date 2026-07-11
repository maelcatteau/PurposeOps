//! Chiffrement des secrets au repos (Phase 8) — schéma détaillé dans `PORTING.md`.
//!
//! Une identité `age` par client (`~/.config/ppo/keys/<client>.txt`), pas une seule clé
//! globale : `db_credentials.password` d'un déploiement est chiffré à un seul destinataire
//! (celui de son client), tandis que `Host.identity_key` (clé SSH privée embarquée) est
//! chiffré à **l'union** des clés de tous les clients ayant un déploiement sur cet hôte —
//! `hosts.yaml` n'étant pas un mapping 1:1 client↔hôte. `age` gère nativement le
//! chiffrement multi-destinataires : un seul ciphertext, plusieurs identités peuvent le
//! déchiffrer indépendamment.
//!
//! Le déchiffrement essaie chaque identité locale présente dans `~/.config/ppo/keys/`
//! jusqu'à ce que l'une d'elles fonctionne — pas besoin de savoir à l'avance quelle clé a
//! chiffré quoi.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use age::secrecy::ExposeSecret;
use age::x25519::{Identity, Recipient};
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::{config, deployment};

/// Préfixe qui distingue une valeur chiffrée d'une valeur en clair dans le YAML.
pub const MARKER: &str = "enc:";

pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(MARKER)
}

fn keys_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("$HOME non défini");
    PathBuf::from(home).join(".config/ppo/keys")
}

fn customer_key_path(customer: &str) -> PathBuf {
    keys_dir().join(format!("{customer}.txt"))
}

/// Identité dédiée à un déploiement précis (`agent-<deployment_id>.txt`), distincte de
/// l'identité du client — voir Phase 11.3 dans PORTING.md : un agent de backup poussé sur
/// l'hôte d'un déploiement ne reçoit que cette identité, pas celle du client (qui
/// déchiffrerait tous ses secrets, sur tout le parc). Préfixe `agent-` pour ne jamais
/// collisionner avec un nom de client.
/// `pub(crate)`, contrairement à `customer_key_path` : `backup_agent.rs` a besoin du
/// chemin lui-même (pas seulement de l'identité chargée) pour lire le fichier généré et
/// en pousser le contenu tel quel vers l'hôte distant.
pub(crate) fn agent_identity_path(deployment_id: &str) -> PathBuf {
    keys_dir().join(format!("agent-{deployment_id}.txt"))
}

/// Charge la clé d'un client si elle existe déjà sur cette machine, sinon `None`.
pub fn load_customer_identity(customer: &str) -> Result<Option<Identity>> {
    let path = customer_key_path(customer);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(load_identity_file(&path)?))
}

/// Charge la clé d'un client, la génère (et l'écrit sur disque, `chmod 600`) si elle
/// n'existe pas encore.
pub fn load_or_generate_customer_identity(customer: &str) -> Result<Identity> {
    if let Some(identity) = load_customer_identity(customer)? {
        return Ok(identity);
    }
    let identity = Identity::generate();
    save_identity_file(&identity, &customer_key_path(customer))?;
    Ok(identity)
}

/// Même mécanique que `load_or_generate_customer_identity`, pour l'identité scopée d'un
/// agent de backup. `load_all_local_identities` n'a besoin d'aucun changement : elle
/// charge déjà tout fichier `.txt` de `~/.config/ppo/keys/` quel que soit son nom, donc
/// une identité d'agent est prise en compte par `reveal()` dès qu'elle est présente.
pub fn load_or_generate_agent_identity(deployment_id: &str) -> Result<Identity> {
    let path = agent_identity_path(deployment_id);
    if path.exists() {
        return load_identity_file(&path);
    }
    let identity = Identity::generate();
    save_identity_file(&identity, &path)?;
    Ok(identity)
}

fn load_identity_file(path: &Path) -> Result<Identity> {
    let text = fs::read_to_string(path).with_context(|| format!("lecture de {}", path.display()))?;
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .ok_or_else(|| anyhow!("fichier de clé vide : {}", path.display()))?;
    line.trim()
        .parse::<Identity>()
        .map_err(|e| anyhow!("clé age invalide dans {} : {e}", path.display()))
}

fn save_identity_file(identity: &Identity, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let secret = identity.to_string();
    fs::write(path, format!("{}\n", secret.expose_secret()))
        .with_context(|| format!("écriture de {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Toutes les identités présentes localement dans `~/.config/ppo/keys/*.txt` — utilisées
/// pour essayer de déchiffrer n'importe quel champ, quel que soit le client concerné.
pub fn load_all_local_identities() -> Result<Vec<Identity>> {
    let dir = keys_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut identities = vec![];
    for entry in fs::read_dir(&dir).with_context(|| format!("lecture de {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("txt") {
            identities.push(load_identity_file(&path)?);
        }
    }
    Ok(identities)
}

/// Chiffre `plaintext` pour un ensemble de destinataires (1 pour un secret de client, N
/// pour la clé SSH d'un hôte partagé). Retourne la valeur préfixée `enc:`, prête à être
/// stockée dans le YAML.
pub fn encrypt_secret(plaintext: &str, recipients: &[Recipient]) -> Result<String> {
    if recipients.is_empty() {
        bail!("aucun destinataire pour le chiffrement");
    }
    let dyn_recipients: Vec<&dyn age::Recipient> =
        recipients.iter().map(|r| r as &dyn age::Recipient).collect();
    let encryptor = age::Encryptor::with_recipients(dyn_recipients.into_iter())
        .map_err(|e| anyhow!("chiffrement age : {e}"))?;

    let mut encrypted = vec![];
    let mut writer = encryptor.wrap_output(&mut encrypted)?;
    writer.write_all(plaintext.as_bytes())?;
    writer.finish()?;

    Ok(format!("{MARKER}{}", BASE64.encode(&encrypted)))
}

/// Déchiffre une valeur `enc:...` en essayant chaque identité locale jusqu'à ce que l'une
/// d'elles fonctionne. Erreur explicite si la valeur n'est pas chiffrée, ou si aucune
/// identité locale ne peut la déchiffrer.
pub fn decrypt_secret(value: &str, identities: &[Identity]) -> Result<String> {
    let b64 = value
        .strip_prefix(MARKER)
        .ok_or_else(|| anyhow!("valeur non chiffrée (pas de préfixe '{MARKER}')"))?;
    let bytes = BASE64.decode(b64).context("base64 invalide")?;

    let decryptor =
        age::Decryptor::new(&bytes[..]).map_err(|e| anyhow!("en-tête age invalide : {e}"))?;
    let dyn_identities: Vec<&dyn age::Identity> =
        identities.iter().map(|i| i as &dyn age::Identity).collect();
    let mut reader = decryptor
        .decrypt(dyn_identities.into_iter())
        .map_err(|_| anyhow!("aucune clé locale ne peut déchiffrer cette valeur"))?;

    let mut plaintext = vec![];
    reader.read_to_end(&mut plaintext)?;
    String::from_utf8(plaintext).context("contenu déchiffré non-UTF8")
}

/// Accesseur paresseux utilisé aux points d'usage réels d'un secret (backup, SSH...) :
/// si `value` est chiffrée, la déchiffre avec les identités locales disponibles ; sinon
/// la renvoie telle quelle (valeurs encore en clair, avant migration — voir 8.4). C'est
/// ce point d'appel, pas `load_hosts`/`load_customers`, qui doit échouer si un secret
/// est illisible : les commandes qui ne touchent pas ce champ ne doivent pas en pâtir.
pub fn reveal(value: &str) -> Result<String> {
    if !is_encrypted(value) {
        return Ok(value.to_string());
    }
    let identities = load_all_local_identities()?;
    decrypt_secret(value, &identities)
}

/// `secrets encrypt` — migration en place de la config réelle : chiffre tout
/// `db_credentials.password` encore en clair (à la clé du client propriétaire du
/// déploiement, générée si besoin) et (ré)chiffre `identity_key` pour chaque hôte
/// (réutilise `ensure_host_key_encrypted`, la même logique déjà exercée par `cdep`).
/// Idempotent au sens du contenu déchiffrable (les valeurs déjà chiffrées ne sont pas
/// touchées) ; réexécuter la commande ne fait que rattraper ce qui reste en clair.
pub fn cmd_secrets_encrypt() -> Result<()> {
    let mut customers = config::load_customers()?;
    let mut hosts = config::load_hosts()?;

    let mut password_count = 0;
    let mut customers_changed = false;

    for (customer_name, cust) in customers.iter_mut() {
        for dep in cust.deployments.iter_mut() {
            let Some(creds) = dep.db_credentials.as_mut() else {
                continue;
            };
            if is_encrypted(&creds.password) {
                continue;
            }
            let identity = load_or_generate_customer_identity(customer_name)?;
            match encrypt_secret(&creds.password, &[identity.to_public()]) {
                Ok(enc) => {
                    creds.password = enc;
                    password_count += 1;
                    customers_changed = true;
                }
                Err(e) => println!(
                    "⚠️  Échec du chiffrement du mot de passe DB de '{customer_name}'/'{}' : {e}",
                    dep.deployment_id
                ),
            }
        }
    }

    let mut host_count = 0;
    let mut hosts_changed = false;
    let host_ids: Vec<String> = hosts.keys().cloned().collect();
    for host_id in host_ids {
        if deployment::ensure_host_key_encrypted(&host_id, &mut hosts, &customers) {
            host_count += 1;
            hosts_changed = true;
        }
    }

    if customers_changed {
        config::save_yaml_map(&config::customers_config_path(), &customers)?;
    }
    if hosts_changed {
        config::save_yaml_map(&config::hosts_config_path(), &hosts)?;
    }

    if password_count == 0 && host_count == 0 {
        println!("ℹ️  Rien à chiffrer, tout est déjà à jour.");
    } else {
        println!(
            "✅ {password_count} mot(s) de passe DB chiffré(s), {host_count} clé(s) SSH d'hôte chiffrée(s)/mise(s) à jour."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
