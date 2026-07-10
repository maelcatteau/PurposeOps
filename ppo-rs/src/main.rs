// ppo-rs — portage Rust de PurposeOps. Plan : ../PORTING.md
//
// Phases 1–2 : prompt + couche config + commandes de lecture/sélection.
// Dépendances : clap (derive) · serde (derive) · serde_yaml_ng · inquire · anyhow

mod check;
mod config;
mod customer;
mod deployment;
mod host;
mod prompt;
mod service;
mod ui;

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
    /// Valide la cohérence de toute la config.
    Check,

    /// Hôte courant (record détaillé).
    #[command(visible_alias = "h")]
    GetCurrentHost,
    /// Nom (id) de l'hôte courant.
    #[command(visible_alias = "hname")]
    HostName,
    /// Liste tous les hôtes configurés.
    #[command(visible_alias = "lsh")]
    ListHosts,
    /// Sélectionne l'hôte courant (arg direct ou menu fuzzy).
    #[command(visible_alias = "sh")]
    SetHost { host_id: Option<String> },

    /// Client courant (nom + abréviation).
    #[command(visible_alias = "c")]
    GetCurrentCustomer,
    /// Nom du client courant.
    #[command(visible_alias = "cname")]
    CustomerName,
    /// Liste tous les clients.
    #[command(visible_alias = "lsc")]
    ListCustomers,
    /// Sélectionne le client courant (arg direct ou menu fuzzy).
    #[command(visible_alias = "sc")]
    SetCustomer { customer: Option<String> },

    /// Id du déploiement courant.
    #[command(visible_alias = "pde")]
    GetCurrentDeployment,
    /// Record complet du déploiement courant.
    #[command(visible_alias = "pdei")]
    GetCurrentDeploymentInfo,
    /// Liste les déploiements du client courant.
    #[command(visible_alias = "lsd")]
    ListDeployments,
    /// Sélectionne le déploiement courant (arg direct ou menu fuzzy).
    #[command(visible_alias = "sd")]
    SetDeployment { deployment_id: Option<String> },

    /// Liste tous les services disponibles.
    #[command(visible_alias = "lss")]
    ListServices,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Prompt => println!("{}", prompt::get_prompt_context()),
        Command::TogglePrompt => prompt::toggle_prompt()?,
        Command::Check => check::cmd_check()?,

        Command::GetCurrentHost => host::cmd_h()?,
        Command::HostName => host::cmd_hname()?,
        Command::ListHosts => host::cmd_lsh()?,
        Command::SetHost { host_id } => host::cmd_sh(host_id)?,

        Command::GetCurrentCustomer => customer::cmd_c()?,
        Command::CustomerName => customer::cmd_cname()?,
        Command::ListCustomers => customer::cmd_lsc()?,
        Command::SetCustomer { customer } => customer::cmd_sc(customer)?,

        Command::GetCurrentDeployment => deployment::cmd_pde()?,
        Command::GetCurrentDeploymentInfo => deployment::cmd_pdei()?,
        Command::ListDeployments => deployment::cmd_lsd()?,
        Command::SetDeployment { deployment_id } => deployment::cmd_sd(deployment_id)?,

        Command::ListServices => service::cmd_lss()?,
    }
    Ok(())
}
