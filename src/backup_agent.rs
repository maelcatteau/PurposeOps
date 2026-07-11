//! `backup bootstrap-agent` — installe un agent de sauvegarde autonome sur l'hôte d'un
//! déploiement (Phase 11.4, voir PORTING.md) : pousse le binaire `ppo` lui-même, une
//! config scopée à CE SEUL déploiement (hôte marqué `localhost` de son propre point de
//! vue), une identité `age` dédiée, et une entrée cron locale — pour que les sauvegardes
//! tournent depuis le cron de l'hôte du client plutôt que celui du laptop (qui peut être
//! éteint/endormi). Capacité entièrement nouvelle, aucun équivalent côté nu.
//!
//! Aucune modification n'est nécessaire dans `backup.rs` : chaque point de dispatch
//! distant (`docker::run_docker_command`, `backup.rs::exec_remote_shell`,
//! `ssh::exec_shell`) vérifie déjà `host.hostname == "localhost"` en premier et prend un
//! chemin d'exécution locale pure avant de toucher quoi que ce soit lié à SSH/ControlMaster
//! — un `Host` scopé avec `hostname: "localhost"` suffit donc à faire tourner
//! `do_generic_backup`/`run_backup_steps` entièrement sur place, tel quel.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result, anyhow, bail};

use crate::config::{self, Context, Customer, CustomerLite, Deployment, DeploymentField, Host};
use crate::{provision, secrets, ssh, ui};

/// `Host` scopé pour l'agent : `hostname` forcé à `localhost` (ce qui fait
/// automatiquement prendre à `backup.rs`/`docker.rs`/`ssh.rs` leurs branches d'exécution
/// locale, sans aucune modification de ce code), champs liés à l'accès SSH distant vidés —
/// un `Host` nommé `localhost` n'atteint jamais la résolution d'identité SSH, inutile d'y
/// dupliquer la clé de la VPS elle-même.
fn build_scoped_host(host: &Host) -> Host {
    Host {
        name: host.name.clone(),
        hostname: "localhost".to_string(),
        user: host.user.clone(),
        port: String::new(),
        identity_file: String::new(),
        arch: host.arch.clone(),
        docker_context: "default".to_string(),
        identity_key: None,
    }
}

/// `customers.yaml` scopé : un seul client, un seul déploiement (celui ciblé), même si ce
/// client en a d'autres ailleurs sur le parc. `hosts` recopié tel quel — inoffensif, garde
/// `ppo check`/`pdei` cohérents si on se connecte pour déboguer, pas requis par
/// `backup run` lui-même.
fn build_scoped_customers(
    customer_name: &str,
    customer: &Customer,
    dep: &Deployment,
) -> BTreeMap<String, Customer> {
    let scoped = Customer {
        abbreviation: customer.abbreviation.clone(),
        deployments: vec![dep.clone()],
        hosts: customer.hosts.clone(),
    };
    BTreeMap::from([(customer_name.to_string(), scoped)])
}

/// `context.yaml` scopé : le déploiement pré-sélectionné, comme `sd` l'écrirait.
fn build_scoped_context(
    host_id: &str,
    scoped_host: &Host,
    customer_name: &str,
    customer: &Customer,
    dep: &Deployment,
) -> Context {
    Context {
        host: BTreeMap::from([(host_id.to_string(), scoped_host.clone())]),
        prompt_show: false,
        customer: BTreeMap::from([(
            customer_name.to_string(),
            CustomerLite { abbreviation: customer.abbreviation.clone() },
        )]),
        deployment: Some(DeploymentField::Full(Box::new(dep.clone()))),
    }
}

/// Contenu complet du fichier `/etc/cron.d/ppo-backup-<deployment_id>`. Un fichier
/// `cron.d` peut porter des lignes d'affectation de variable d'environnement
/// (`NTFY_URL=...`) qui s'appliquent à toutes les tâches qu'il contient ;
/// `backup::cmd_backup_run` la lit au runtime pour notifier un échec. Réécrit en entier à
/// chaque bootstrap : la ré-exécution de `bootstrap-agent` est la façon prévue de faire
/// tourner le binaire/la config/le topic ntfy poussés.
fn build_cron_line(
    deployment_id: &str,
    user: &str,
    binary_path: &str,
    ntfy_url: Option<&str>,
    keep_last: u32,
) -> String {
    let mut out = String::new();
    out.push_str("SHELL=/bin/sh\n");
    out.push_str("PATH=/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin\n");
    if let Some(url) = ntfy_url {
        out.push_str(&format!("NTFY_URL={url}\n"));
    }
    out.push_str(&format!(
        "7 3 * * * {user} {binary_path} backup run --cron --keep-last {keep_last} >> {}/backups/ppo-backup-agent.log 2>&1\n",
        ssh::remote_home(user)
    ));
    out.push_str(&format!("# géré par `ppo backup bootstrap-agent {deployment_id}` — ne pas éditer à la main\n"));
    out
}

/// Cherche un déploiement par id dans tous les clients (ids globaux, même contrainte que
/// `deployment::deployment_id_exists`).
fn find_deployment_globally<'a>(
    deployment_id: &str,
    customers: &'a BTreeMap<String, Customer>,
) -> Option<(&'a str, &'a Customer, &'a Deployment)> {
    for (name, cust) in customers {
        if let Some(dep) = cust.deployments.iter().find(|d| d.deployment_id == deployment_id) {
            return Some((name, cust, dep));
        }
    }
    None
}

/// Sélection interactive parmi TOUS les déploiements de TOUS les clients — cette commande
/// est un outil de mise en place ponctuelle, elle ne doit pas dépendre du client/
/// déploiement actuellement sélectionné dans le contexte de session.
fn select_deployment_globally(customers: &BTreeMap<String, Customer>) -> Option<String> {
    let mut ids: Vec<String> = customers
        .values()
        .flat_map(|c| c.deployments.iter().map(|d| d.deployment_id.clone()))
        .collect();
    ids.sort();
    ui::select("Déploiement à autonomiser :", ids)
}

/// L'architecture de l'hôte (`x86_64`/`arm64`, convention du projet) correspond-elle à
/// celle de la machine qui exécute `ppo bootstrap-agent` (`std::env::consts::ARCH`, qui
/// utilise `aarch64` plutôt que `arm64`) ?
fn host_matches_local_arch(host_arch: &str) -> bool {
    let local = std::env::consts::ARCH;
    match host_arch {
        "arm64" => local == "aarch64",
        other => local == other,
    }
}

/// Compile `ppo` en release et lit le binaire produit. Pas de compilation croisée
/// automatisée (Phase 11.6, voir PORTING.md) : seul le cas même-architecture est couvert
/// ici, le cas contraire est rejeté avant d'arriver jusque-là (`host_matches_local_arch`).
fn build_release_binary() -> Result<Vec<u8>> {
    println!("🔨 Compilation du binaire ppo (release)...");
    let status = std::process::Command::new("cargo").args(["build", "--release"]).status()?;
    if !status.success() {
        bail!("Échec de 'cargo build --release'");
    }
    let path = std::path::Path::new("target/release/ppo");
    std::fs::read(path).with_context(|| format!("lecture de {}", path.display()))
}

/// (Ré)chiffre `db_credentials.password` du déploiement pour DEUX destinataires : le
/// client (comme d'habitude) ET l'identité scopée de cet agent — voir la décision de
/// conception dans PORTING.md Phase 11.3 (ne jamais pousser l'identité RÉELLE du client
/// sur l'hôte d'un déploiement, qui déchiffrerait tout son historique de secrets, pas
/// seulement celui-ci). Mute le `customers.yaml` réel : même catégorie d'action que la
/// migration Phase 8.4, à vérifier contre des données de test avant tout déploiement
/// réel.
fn ensure_agent_recipient(
    customer_name: &str,
    deployment_id: &str,
    customers: &mut BTreeMap<String, Customer>,
) -> Result<()> {
    let cust = customers
        .get(customer_name)
        .ok_or_else(|| anyhow!("Client '{customer_name}' introuvable"))?;
    let dep = cust
        .deployments
        .iter()
        .find(|d| d.deployment_id == deployment_id)
        .ok_or_else(|| anyhow!("Déploiement '{deployment_id}' introuvable"))?;
    let creds = dep.db_credentials.as_ref().ok_or_else(|| {
        anyhow!("Déploiement '{deployment_id}' sans db_credentials — rien à autonomiser")
    })?;

    let plaintext = secrets::reveal(&creds.password).context("déchiffrement du mot de passe DB")?;
    let customer_identity = secrets::load_or_generate_customer_identity(customer_name)?;
    let agent_identity = secrets::load_or_generate_agent_identity(deployment_id)?;
    let encrypted = secrets::encrypt_secret(
        &plaintext,
        &[customer_identity.to_public(), agent_identity.to_public()],
    )?;

    let cust = customers.get_mut(customer_name).expect("vérifié ci-dessus");
    let dep = cust
        .deployments
        .iter_mut()
        .find(|d| d.deployment_id == deployment_id)
        .expect("vérifié ci-dessus");
    dep.db_credentials.as_mut().expect("vérifié ci-dessus").password = encrypted;

    config::save_yaml_map(&config::customers_config_path(), customers)?;
    println!("🔐 Mot de passe DB chiffré pour le client ET l'agent de '{deployment_id}'.");
    Ok(())
}

/// `backup bootstrap-agent` — voir le doc du module pour la vue d'ensemble.
pub fn cmd_backup_bootstrap_agent(
    deployment_id: Option<String>,
    ntfy_url: Option<String>,
    keep_last: u32,
) -> Result<()> {
    let mut customers = config::load_customers()?;

    let deployment_id = match deployment_id {
        Some(id) => id,
        None => select_deployment_globally(&customers)
            .ok_or_else(|| anyhow!("Aucun déploiement sélectionné."))?,
    };

    let (customer_name, _, dep) = find_deployment_globally(&deployment_id, &customers)
        .ok_or_else(|| anyhow!("Déploiement '{deployment_id}' introuvable"))?;
    let customer_name = customer_name.to_string();
    if dep.db_credentials.is_none() {
        bail!(
            "Déploiement '{deployment_id}' sans base de données — un agent de backup suppose \
            un `db_credentials` configuré (voir 'ppo cdep')."
        );
    }
    let host_id = dep
        .hosts
        .first()
        .map(|h| h.host_id.clone())
        .ok_or_else(|| anyhow!("Déploiement '{deployment_id}' sans hôte."))?;

    let hosts = config::load_hosts()?;
    // Connexion : le VRAI hôte de hosts.yaml. Le `Host` scopé (hostname: "localhost")
    // n'est que de la DONNÉE écrite dans le fichier distant, jamais utilisé pour ouvrir
    // la connexion — piège facile, d'où ce commentaire.
    let real_host = hosts
        .get(&host_id)
        .ok_or_else(|| anyhow!("Hôte '{host_id}' introuvable dans hosts.yaml"))?
        .clone();

    if !host_matches_local_arch(&real_host.arch) {
        bail!(
            "L'hôte '{host_id}' a arch='{}', différente de cette machine ('{}'). La compilation \
            croisée n'est pas encore automatisée (PORTING.md Phase 11.6) : installez la cible \
            avec `rustup target add <triple>` et un linker croisé (ex. `apt-get install \
            gcc-aarch64-linux-gnu` pour arm64), compilez avec `cargo build --release --target \
            <triple>`, puis relancez cette commande.",
            real_host.arch,
            std::env::consts::ARCH
        );
    }

    // `docker::run_docker_command`'s remote branch never prepends `sudo` — it assumes the
    // SSH user is already in the `docker` group, true today only because every existing
    // host was provisioned manually before `ppo` existed. A host freshly set up via
    // `ppo bootstrap` (Phase 10) has no such membership: `get.docker.com`'s installer only
    // *suggests* `usermod -aG docker`, it doesn't do it. Without this, the agent's own
    // (non-sudo) docker calls would fail with "permission denied" on every real cron run.
    // Group membership only takes effect for a *new* login session, not any SSH
    // connection already open — irrelevant here since this command's own steps below
    // never call `docker` themselves, only the cron-spawned agent does, and cron always
    // starts a fresh session.
    ssh::exec_shell_checked(
        &real_host,
        &format!("sudo usermod -aG docker '{}'", real_host.user),
        "ajout de l'utilisateur SSH au groupe docker",
    )?;

    ensure_agent_recipient(&customer_name, &deployment_id, &mut customers)?;

    // Recharge : `ensure_agent_recipient` vient de réécrire customers.yaml, on veut le
    // `db_credentials.password` désormais chiffré pour l'agent dans la config poussée.
    let customers = config::load_customers()?;
    let (_, customer, dep) = find_deployment_globally(&deployment_id, &customers)
        .ok_or_else(|| anyhow!("Déploiement '{deployment_id}' introuvable après mise à jour"))?;
    let customer = customer.clone();
    let dep = dep.clone();

    let binary_bytes = build_release_binary()?;

    let scoped_host = build_scoped_host(&real_host);
    let scoped_customers = build_scoped_customers(&customer_name, &customer, &dep);
    let scoped_context =
        build_scoped_context(&host_id, &scoped_host, &customer_name, &customer, &dep);

    let remote_config_dir =
        ssh::resolve_remote_path("~/dev/nu-modules/PurposeOps/PurposeOps-config", &real_host.user);
    let remote_bin_path =
        ssh::resolve_remote_path("~/dev/nu-modules/PurposeOps/target/release/ppo", &real_host.user);
    let remote_agent_key = ssh::resolve_remote_path(
        &format!("~/.config/ppo/keys/agent-{deployment_id}.txt"),
        &real_host.user,
    );

    println!("📤 Envoi de la config scopée vers '{host_id}'...");
    provision::push_file(
        &real_host,
        &format!("{remote_config_dir}/hosts.yaml"),
        &serde_yaml_ng::to_string(&BTreeMap::from([(host_id.clone(), scoped_host)]))?,
    )?;
    provision::push_file(
        &real_host,
        &format!("{remote_config_dir}/customers.yaml"),
        &serde_yaml_ng::to_string(&scoped_customers)?,
    )?;
    provision::push_file(
        &real_host,
        &format!("{remote_config_dir}/context.yaml"),
        &serde_yaml_ng::to_string(&scoped_context)?,
    )?;

    println!("🔑 Envoi de l'identité de l'agent...");
    let agent_identity_text =
        std::fs::read_to_string(secrets::agent_identity_path(&deployment_id))
            .context("lecture de l'identité de l'agent générée localement")?;
    provision::push_file(&real_host, &remote_agent_key, &agent_identity_text)?;
    ssh::exec_shell_checked(
        &real_host,
        &format!("chmod 600 '{remote_agent_key}'"),
        "chmod 600 de l'identité de l'agent",
    )?;

    println!("📦 Envoi du binaire ppo ({} octets)...", binary_bytes.len());
    provision::push_binary(&real_host, &remote_bin_path, &binary_bytes)?;

    println!("🔎 Vérification du binaire distant...");
    ssh::exec_shell_checked(
        &real_host,
        &format!("{remote_bin_path} --help"),
        "vérification du binaire distant (--help)",
    )?;

    println!("⏰ Installation de la tâche cron...");
    let cron_content = build_cron_line(
        &deployment_id,
        &real_host.user,
        &remote_bin_path,
        ntfy_url.as_deref(),
        keep_last,
    );
    let tmp_cron_path =
        ssh::resolve_remote_path(&format!("~/.ppo-cron-{deployment_id}.tmp"), &real_host.user);
    provision::push_file(&real_host, &tmp_cron_path, &cron_content)?;
    ssh::exec_shell_checked(
        &real_host,
        &format!(
            "sudo install -m 0644 '{tmp_cron_path}' '/etc/cron.d/ppo-backup-{deployment_id}' && rm -f '{tmp_cron_path}'"
        ),
        "installation de la tâche cron",
    )?;

    println!("✅ Agent de backup installé pour '{deployment_id}' sur '{host_id}'.");
    println!("   Binaire      : {remote_bin_path}");
    println!("   Cron         : /etc/cron.d/ppo-backup-{deployment_id}");
    println!(
        "   Notification : {}",
        ntfy_url.as_deref().unwrap_or("(aucune — pas de --ntfy-url fourni)")
    );
    Ok(())
}

#[cfg(test)]
mod tests;
