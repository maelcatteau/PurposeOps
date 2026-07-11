//! Rendu de templates docker-compose (port de `templater.nu`).
//!
//! Le dossier des templates est codé en dur (`~/dev/nu-modules/PurposeOps/templates`,
//! comme `templater.nu`'s `get-template-dir-path`) — les champs `template_dir_path`/
//! `template_compose_path` de `services.yaml` existent mais ne sont **jamais lus** par
//! `generate-compose` côté nu (vérifié dans le code source), donc pas repris ici non
//! plus : parité de comportement, pas du schéma déclaré.
//!
//! Les champs `parent`/`type`/`required`/`validation`/`default_pattern` du YAML d'une
//! variable existent dans `templates/<Service>/template.yml` (ex. Vaultwarden) mais ne
//! sont **jamais lus** par `generate-compose` côté nu non plus — schéma partiellement
//! aspirationnel, pas repris ici.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

use crate::{config, ui};

fn templates_dir() -> PathBuf {
    config::base_path().join("templates")
}

#[derive(Debug, Clone, Deserialize)]
struct VariableDef {
    #[serde(default)]
    level: i64,
    #[serde(default)]
    description: String,
    #[serde(default)]
    example: Option<serde_yaml_ng::Value>,
}

/// `variables` reste une `Mapping` (pas une `BTreeMap`, qui réordonnerait alphabétiquement)
/// pour préserver l'ordre de déclaration du YAML : le tri par `level` plus bas est stable,
/// donc pour des variables de même niveau (ex. `http_port`/`https_port`/`admin_port` dans
/// `templates/Caddy/template.yml`, toutes `level: 2`), l'ordre des prompts doit rester
/// l'ordre du fichier — comme le `sort-by` (stable) du nu sur le `transpose` d'un record,
/// qui préserve lui aussi l'ordre d'origine.
#[derive(Debug, Deserialize)]
struct TemplateDef {
    variables: serde_yaml_ng::Mapping,
}

fn format_example(v: &serde_yaml_ng::Value) -> String {
    match v {
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        other => serde_yaml_ng::to_string(other).unwrap_or_default().trim().to_string(),
    }
}

/// Cas spécial "networks" (nu `templater.nu`) : une variable `networks` (liste séparée
/// par des virgules) génère deux variables dérivées consommées par le template —
/// `networks_section` (liste YAML) et `networks_definition` (déclarations `external:
/// true`). Le join volontairement SANS indentation en tête de la première entrée
/// s'appuie sur l'indentation déjà présente dans le template avant `{{networks_section}}`/
/// `{{networks_definition}}` (voir `templates/Caddy/docker-compose.yml`) — reproduit tel
/// quel, fragile mais fidèle au nu. No-op si aucune variable "networks" n'est présente.
fn expand_networks(variables: &mut BTreeMap<String, String>) {
    let Some(networks) = variables.get("networks").cloned() else {
        return;
    };
    let list: Vec<String> = networks.split(',').map(|n| n.trim().to_string()).collect();
    let section = list
        .iter()
        .map(|n| format!("- {n}"))
        .collect::<Vec<_>>()
        .join("\n      ");
    let definition = list
        .iter()
        .map(|n| format!("{n}:\n    external: true"))
        .collect::<Vec<_>>()
        .join("\n  ");
    variables.insert("networks_section".to_string(), section);
    variables.insert("networks_definition".to_string(), definition);
}

/// Substitue chaque `{{key}}` de `template` par sa valeur dans `variables`. Pure —
/// `expand_networks` doit avoir déjà été appliquée si besoin.
fn substitute(template: &str, variables: &BTreeMap<String, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in variables {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    out
}

/// Port de `generate-compose` : rend `templates/<service_name>/docker-compose.yml` en
/// substituant `{{var}}`. `service_name` désigne le **template** (clé de `services.yaml`,
/// ex. "Vaultwarden") ; `docker_service_name` est le nom de service Docker propre à
/// **cette instance** (ex. "vw-cocotte") — il alimente automatiquement les variables
/// `service_name` et `container_name` du template, dont la saisie est donc sautée même
/// si elles sont déclarées dans `template.yml` (Vaultwarden en déclare une, avec sa
/// propre description/exemple, qui n'est jamais utilisée — même comportement que nu).
///
/// `None` si l'utilisateur annule une saisie (Échap/Ctrl-C).
pub fn generate_compose(service_name: &str, docker_service_name: &str) -> Result<Option<String>> {
    let dir = templates_dir().join(service_name);
    let compose_path = dir.join("docker-compose.yml");
    let variables_path = dir.join("template.yml");

    if !compose_path.exists() {
        bail!("Fichier introuvable : {}", compose_path.display());
    }
    if !variables_path.exists() {
        bail!("Fichier introuvable : {}", variables_path.display());
    }

    let template_content = std::fs::read_to_string(&compose_path)
        .with_context(|| format!("lecture de {}", compose_path.display()))?;
    let variables_text = std::fs::read_to_string(&variables_path)
        .with_context(|| format!("lecture de {}", variables_path.display()))?;
    let def: TemplateDef = serde_yaml_ng::from_str(&variables_text)
        .with_context(|| format!("parsing YAML de {}", variables_path.display()))?;

    println!("📝 Generating compose for docker service: {docker_service_name}");
    println!("Please provide values for the following variables:");

    let mut user_variables: BTreeMap<String, String> = BTreeMap::new();
    user_variables.insert("service_name".to_string(), docker_service_name.to_string());
    user_variables.insert("container_name".to_string(), docker_service_name.to_string());

    let mut sorted_vars: Vec<(String, VariableDef)> = def
        .variables
        .iter()
        .map(|(k, v)| -> Result<(String, VariableDef)> {
            let name = k
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("clé de variable non-string dans template.yml"))?
                .to_string();
            let var_def: VariableDef = serde_yaml_ng::from_value(v.clone())
                .with_context(|| format!("parsing de la variable '{name}'"))?;
            Ok((name, var_def))
        })
        .collect::<Result<Vec<_>>>()?;
    sorted_vars.sort_by_key(|(_, v)| v.level);

    for (name, var_def) in sorted_vars {
        if name == "service_name" || name == "container_name" {
            continue;
        }
        let example_text = match &var_def.example {
            Some(ex) => format!(" (ex: {})", format_example(ex)),
            None => String::new(),
        };
        let prompt = format!("  {}{example_text}: ", var_def.description);
        let Some(value) = ui::text(&prompt) else {
            return Ok(None);
        };
        user_variables.insert(name, value);
    }

    expand_networks(&mut user_variables);
    let final_compose = substitute(&template_content, &user_variables);

    println!("✅ Docker compose generated successfully!");
    Ok(Some(final_compose))
}

#[cfg(test)]
mod tests;
