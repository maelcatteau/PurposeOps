// ppo-rs — portage Rust de PurposeOps. Plan : ../PORTING.md
//
// Phase 1 : binaire de prompt qui remplace le `nu -c '...'` lancé par Starship.
// Dépendances : clap (derive) · serde (derive) · serde_yaml_ng · inquire · anyhow

mod config;
mod prompt;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ppor", version, about = "PurposeOps — port Rust (voir PORTING.md)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Affiche la chaîne de contexte pour le prompt Starship.
    Prompt,
    /// Bascule l'affichage du prompt (on/off).
    #[command(visible_alias = "t")]
    TogglePrompt,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Prompt => println!("{}", prompt::get_prompt_context()),
        Command::TogglePrompt => prompt::toggle_prompt()?,
    }
    Ok(())
}
