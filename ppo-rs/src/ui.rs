//! Helpers d'interaction terminal (équivalent de `config-helper.nu` `select_item` +
//! les prompts `input`). `inquire` fournit le filtre fuzzy nativement, ce qui remplace
//! à la fois `input list --fuzzy` et le fallback `fzf` du code nu.

use inquire::{Confirm, Select};

/// Menu de sélection fuzzy. `None` si liste vide ou si l'utilisateur annule (Échap).
pub fn select(prompt: &str, items: Vec<String>) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    Select::new(prompt, items).prompt().ok()
}

/// Confirmation oui/non (défaut : non). Annulation → `false`.
pub fn confirm(prompt: &str) -> bool {
    Confirm::new(prompt)
        .with_default(false)
        .prompt()
        .unwrap_or(false)
}
