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

#[cfg(test)]
mod tests;
