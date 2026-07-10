//! Logique du prompt Starship (port de `context/prompt/prompt-manager.nu`).

use anyhow::Result;

use crate::config::{self, Context, DeploymentField};

/// Chaîne affichée par Starship. Ne panique jamais : toute erreur de lecture ou
/// incohérence donne "❓ unknown" (comme le `catch` du code nu).
pub fn get_prompt_context() -> String {
    match config::load_context() {
        Ok(ctx) => format_prompt(&ctx),
        Err(_) => "❓ unknown".to_string(),
    }
}

/// Partie pure et testable : contexte → chaîne. Séparée de l'I/O disque pour les tests.
pub fn format_prompt(ctx: &Context) -> String {
    if !ctx.prompt_show {
        return String::new();
    }
    render(ctx).unwrap_or_else(|| "❓ unknown".to_string())
}

/// `None` si l'état est incohérent (aucun hôte sélectionné) → traduit en "❓ unknown".
fn render(ctx: &Context) -> Option<String> {
    // Premier (et unique) hôte du contexte.
    let host = ctx.host.values().next()?;

    // Suffixe déploiement : seulement pour un record complet, pas l'ancien format string.
    let deployment_label = match &ctx.deployment {
        Some(DeploymentField::Full(d)) => format!(" ({})", d.deployment_id),
        _ => String::new(),
    };

    let is_local = host.hostname == "localhost";

    // Le suffixe déploiement n'apparaît que s'il y a aussi un client (fidèle au nu).
    match ctx.customer.values().next() {
        Some(customer) => {
            let abbr = &customer.abbreviation;
            if is_local {
                Some(format!("🏠 local - {abbr}{deployment_label}"))
            } else {
                Some(format!("🌐 {} - {abbr}{deployment_label}", host.name))
            }
        }
        None => {
            if is_local {
                Some("🏠 local".to_string())
            } else {
                Some(format!("🌐 {}", host.name))
            }
        }
    }
}

/// Inverse `prompt_show` dans le contexte et le sauvegarde (alias `t` côté CLI).
pub fn toggle_prompt() -> Result<()> {
    let mut ctx = config::load_context()?;
    ctx.prompt_show = !ctx.prompt_show;
    config::save_context(&ctx)?;
    println!("📍 Context set to prompt_show: {}", ctx.prompt_show);
    Ok(())
}

#[cfg(test)]
mod tests;
