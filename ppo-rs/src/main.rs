// ppo-rs — portage Rust de PurposeOps. Plan : ../PORTING.md
//
// Phases 1–3 : prompt + couche config + lecture/sélection + SSH ControlMaster.
// Dépendances : clap (derive) · serde (derive) · serde_yaml_ng · inquire · anyhow

mod backup;
mod check;
mod config;
mod customer;
mod deployment;
mod docker;
mod host;
mod prompt;
mod service;
mod ssh;
mod table;
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
    /// Crée un nouvel hôte (wizard interactif).
    #[command(visible_alias = "ch")]
    CreateHost,
    /// Supprime un hôte (sélection fuzzy + confirmation).
    #[command(visible_alias = "dh")]
    DeleteHost,

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
    /// Crée un nouveau client (wizard interactif).
    #[command(visible_alias = "cc")]
    CreateCustomer,
    /// Supprime un client (sélection fuzzy + confirmation).
    #[command(visible_alias = "dc")]
    DeleteCustomer,

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
    /// Crée un déploiement pour le client courant (wizard interactif).
    #[command(visible_alias = "cdep")]
    CreateDeployment,

    /// Liste tous les services disponibles.
    #[command(visible_alias = "lss")]
    ListServices,
    /// Crée un nouveau service (wizard interactif).
    #[command(visible_alias = "cs")]
    CreateService,
    /// Supprime un service (sélection fuzzy + confirmation).
    #[command(visible_alias = "ds")]
    DeleteService,

    /// Ferme la connexion SSH maître de l'hôte courant.
    #[command(visible_alias = "close")]
    CloseConnection,
    /// Ferme toutes les connexions SSH maîtres.
    #[command(visible_alias = "closeall")]
    CloseAllConnections,
    /// Liste les connexions SSH maîtres actives.
    #[command(visible_alias = "lsconn")]
    ListConnections,

    /// Démarre un conteneur (sélection fuzzy parmi tous les conteneurs).
    #[command(visible_alias = "dstart")]
    DockerStart,
    /// Arrête un conteneur (sélection fuzzy parmi les conteneurs actifs).
    #[command(visible_alias = "dstop")]
    DockerStop,
    /// Redémarre un conteneur.
    #[command(visible_alias = "drestart")]
    DockerRestart,
    /// Extrait les réseaux Docker d'un conteneur choisi.
    #[command(visible_alias = "dnextract")]
    DockerNetworksExtract,
    /// Statut des conteneurs en cours (filtre regex optionnel).
    #[command(visible_alias = "dps")]
    DockerPs {
        filter: Option<String>,
        #[arg(short, long)]
        ports: bool,
    },
    /// Liste les réseaux Docker (filtre regex optionnel).
    #[command(visible_alias = "dnls")]
    DockerNetworkList { filter: Option<String> },

    /// Sauvegarde / restauration (port de `customer-manager/backup.nu`).
    #[command(subcommand)]
    Backup(BackupCommand),
}

#[derive(Subcommand)]
enum BackupCommand {
    /// Sauvegarde le déploiement courant (dump SQL + filestore) vers l'hôte cible.
    Run {
        /// Préfixe `cron_` au lieu de `manual_` dans le nom de l'archive.
        #[arg(long)]
        cron: bool,
        /// Dossier de sortie sur l'hôte cible (défaut : ~/backups/<abréviation>/<host_id>).
        #[arg(long = "output-dir")]
        output_dir: Option<String>,
    },
    /// Restaure une archive dans le déploiement courant. DESTRUCTIF (DROP DATABASE sur
    /// la cible) : demande confirmation sauf si --force.
    Restore {
        /// Nom de fichier (résolu dans le dossier de backup du client courant) ou chemin
        /// absolu sur l'hôte cible ; si omis, sélection interactive.
        backup_file: Option<String>,
        /// Base de destination (défaut : database_name du déploiement courant).
        #[arg(long = "target-database")]
        target_database: Option<String>,
        /// Ne pas demander de confirmation avant d'écraser la base cible.
        #[arg(long)]
        force: bool,
    },
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
        Command::CreateHost => host::cmd_ch()?,
        Command::DeleteHost => host::cmd_dh()?,

        Command::GetCurrentCustomer => customer::cmd_c()?,
        Command::CustomerName => customer::cmd_cname()?,
        Command::ListCustomers => customer::cmd_lsc()?,
        Command::SetCustomer { customer } => customer::cmd_sc(customer)?,
        Command::CreateCustomer => customer::cmd_cc()?,
        Command::DeleteCustomer => customer::cmd_dc()?,

        Command::GetCurrentDeployment => deployment::cmd_pde()?,
        Command::GetCurrentDeploymentInfo => deployment::cmd_pdei()?,
        Command::ListDeployments => deployment::cmd_lsd()?,
        Command::SetDeployment { deployment_id } => deployment::cmd_sd(deployment_id)?,
        Command::CreateDeployment => deployment::cmd_cdep()?,

        Command::ListServices => service::cmd_lss()?,
        Command::CreateService => service::cmd_cs()?,
        Command::DeleteService => service::cmd_ds()?,

        Command::CloseConnection => ssh::close_current_master_connection()?,
        Command::CloseAllConnections => ssh::close_all_master_connections(),
        Command::ListConnections => ssh::list_master_connections(),

        Command::DockerStart => docker::cmd_start()?,
        Command::DockerStop => docker::cmd_stop()?,
        Command::DockerRestart => docker::cmd_restart()?,
        Command::DockerNetworksExtract => docker::cmd_dn_extract()?,
        Command::DockerPs { filter, ports } => docker::cmd_dps(filter, ports)?,
        Command::DockerNetworkList { filter } => docker::cmd_dnls(filter)?,

        Command::Backup(BackupCommand::Run { cron, output_dir }) => {
            backup::cmd_backup_run(output_dir, cron)?
        }
        Command::Backup(BackupCommand::Restore { backup_file, target_database, force }) => {
            backup::cmd_backup_restore(backup_file, target_database, force)?
        }
    }
    Ok(())
}
