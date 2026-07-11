//! Tests du binaire (`src/main.rs`). `generate_completions` est la seule partie testable
//! sans mocker `inquire`/le réseau : rendu déterministe étant donné un `Cli` et un shell.

use super::*;

#[test]
fn generate_completions_bash_produit_une_sortie_non_vide() {
    let mut buf = Vec::new();
    generate_completions(CompletionShell::Bash, &mut buf);
    assert!(!buf.is_empty());
    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("ppo"), "le script généré doit référencer le nom du binaire");
}

#[test]
fn generate_completions_couvre_chaque_shell_sans_paniquer() {
    let shells = [
        CompletionShell::Bash,
        CompletionShell::Elvish,
        CompletionShell::Fish,
        CompletionShell::PowerShell,
        CompletionShell::Zsh,
        CompletionShell::Nushell,
    ];
    for shell in shells {
        let mut buf = Vec::new();
        generate_completions(shell, &mut buf);
        assert!(!buf.is_empty());
    }
}
