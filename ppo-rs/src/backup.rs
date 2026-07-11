//! `backup run` / `backup restore` (port de `customer-manager/backup.nu`'s `backup run`
//! + `backup restore` + `do-generic-backup` + `do-generic-restore`).
//!
//! `backup restore` est destructif (`DROP DATABASE` sur la cible) et peut restaurer une
//! archive venant d'un tout autre client/déploiement (croisement backup/restore : les
//! fichiers `.sql`/`_fs.tar.gz` de l'archive portent le nom de la base D'ORIGINE, pas
//! forcément celui de la cible) — voir `run_restore_steps`.
//!
//! Écarts volontaires avec le nu, purement du nettoyage — comportement inchangé :
//! - `--service` (jamais lu dans le corps de `backup run` côté nu) et `--silent` (jamais
//!   lu ni dans `backup run`/`backup restore` ni dans `do-generic-backup`/
//!   `do-generic-restore`) sont des paramètres morts côté nu ; ils ne sont pas repris ici.
//! - `--dbHost` est accepté par `do-generic-backup`/`do-generic-restore` côté nu mais
//!   jamais utilisé dans les commandes `pg_dump`/`psql` (qui forcent `-h localhost`,
//!   cohérent puisqu'elles tournent *dans* le conteneur DB) : le paramètre correspondant
//!   n'est pas non plus repris ici.
//! - Le bloc `🔍 DEBUG VARIABLES` de `backup run` (dump de variables internes) était du
//!   débogage laissé en place, pas un comportement voulu : pas porté.

use std::process::{Command, Output};

use anyhow::{Context as _, Result, anyhow, bail};

use crate::config::{self, Host};
use crate::{customer, deployment, docker, secrets, ssh, ui};

/// Le nu remplace en dur `~` par le home de l'utilisateur SSH distant (pas celui du
/// laptop) : un `~/...` entre quotes simples dans une commande shell distante n'est de
/// toute façon jamais tilde-expansé par le shell. Voir la note dans CLAUDE.md.
const REMOTE_HOME: &str = "/home/ngner";

fn resolve_remote_path(path: &str) -> String {
    path.replace('~', REMOTE_HOME)
}

fn stdout_str(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn stderr_str(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

/// Commande `docker` sur l'hôte cible (local ou distant, via `run_docker_command`).
fn exec_remote(args: &[&str], host: &Host) -> Result<Output> {
    docker::run_docker_command(args, host)
}

/// Commande shell brute (non-docker) sur l'hôte cible : `sh -c` en local, sinon la
/// connexion SSH ControlMaster.
fn exec_remote_shell(cmd: &str, host: &Host) -> Result<Output> {
    if host.hostname == "localhost" {
        Ok(Command::new("sh").arg("-c").arg(cmd).output()?)
    } else {
        ssh::run_with_master(host, cmd)
    }
}

fn check_step(result: &Output, step: &str) -> Result<()> {
    if result.status.success() {
        return Ok(());
    }
    let code = result
        .status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "signal".to_string());
    let stderr = stderr_str(result);
    let stdout = stdout_str(result);
    println!("❌ Échec à l'étape '{step}' (code {code})");
    if !stderr.is_empty() {
        println!("   stderr : {stderr}");
    }
    if !stdout.is_empty() {
        println!("   stdout : {stdout}");
    }
    bail!("Échec de l'étape '{step}': {stderr}");
}

fn exec_remote_checked(args: &[&str], host: &Host, step: &str) -> Result<Output> {
    let result = exec_remote(args, host)?;
    check_step(&result, step)?;
    Ok(result)
}

fn exec_remote_shell_checked(cmd: &str, host: &Host, step: &str) -> Result<Output> {
    let result = exec_remote_shell(cmd, host)?;
    check_step(&result, step)?;
    Ok(result)
}

/// Liste les backups (`*.tar.gz`) disponibles dans un dossier distant, du plus récent
/// au plus ancien.
fn list_remote_backups(dir: &str, host: &Host) -> Result<Vec<String>> {
    let result = exec_remote_shell(&format!("ls -1t '{dir}' 2>/dev/null"), host)?;
    Ok(String::from_utf8_lossy(&result.stdout)
        .lines()
        .filter(|l| l.ends_with(".tar.gz"))
        .map(str::to_string)
        .collect())
}

/// Horodatage local (heure du laptop, comme `date now` côté nu) — un `sh`/`date`
/// externe plutôt qu'ajouter une dépendance de calendrier pour un seul usage cosmétique
/// (désambiguïser des noms de fichiers).
fn local_timestamp() -> Result<String> {
    let output = Command::new("date").arg("+%Y%m%d_%H%M%S").output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// `backup run` — sauvegarde le déploiement courant (dump SQL + filestore) vers l'hôte cible.
pub fn cmd_backup_run(output_dir: Option<String>, cron: bool) -> Result<()> {
    let (customer_name, _) =
        customer::get_current_customer()?.ok_or_else(|| anyhow!("❌ Aucun client sélectionné."))?;
    let customers = config::load_customers()?;
    let customer_data = customers
        .get(&customer_name)
        .ok_or_else(|| anyhow!("Client '{customer_name}' introuvable"))?;

    let dep = deployment::get_current_deployment_info()?;
    let host_id = dep
        .hosts
        .first()
        .map(|h| h.host_id.clone())
        .ok_or_else(|| anyhow!("Déploiement sans hôte."))?;
    let database = dep.database_name.clone().ok_or_else(|| {
        anyhow!("❌ Ce déploiement n'a pas de base de données configurée (database_name manquant).")
    })?;

    let hosts = config::load_hosts()?;
    let host = hosts
        .get(&host_id)
        .ok_or_else(|| anyhow!("Hôte '{host_id}' introuvable dans hosts.yaml"))?;

    let app_container = dep
        .container_name
        .clone()
        .ok_or_else(|| anyhow!("❌ Ce déploiement n'a pas de container_name configuré."))?;
    let db_container = dep
        .db_container_name
        .clone()
        .ok_or_else(|| anyhow!("❌ Ce déploiement n'a pas de db_container_name configuré."))?;

    println!("📦 Conteneur identifié : {app_container}");

    let creds = dep.db_credentials.as_ref().ok_or_else(|| {
        anyhow!("❌ Credentials DB manquants. Ajoutez 'db_credentials' dans customers.yaml.")
    })?;
    let db_password = secrets::reveal(&creds.password)
        .context("déchiffrement du mot de passe DB")?;

    println!("✅ Credentials chargés : User={}, Host={}", creds.user, creds.host);

    let final_output_dir = match output_dir.filter(|d| !d.is_empty()) {
        Some(d) => d,
        None => {
            let abbrev = &customer_data.abbreviation;
            if abbrev.is_empty() {
                bail!("Abréviation client manquante.");
            }
            format!("~/backups/{abbrev}/{host_id}")
        }
    };

    println!("📁 Dossier de backup cible sur le serveur : {final_output_dir}");
    println!("🚀 Backup en cours...");

    do_generic_backup(
        &database,
        &app_container,
        &db_container,
        host,
        &creds.port,
        &creds.user,
        &db_password,
        &final_output_dir,
        cron,
    )
}

/// Moteur interne : dump SQL + filestore, archive, rapatriement sur l'hôte, nettoyage.
/// Toute erreur pendant les étapes déclenche un nettoyage best-effort (comme le
/// `try`/`catch` du nu) avant d'être propagée.
#[allow(clippy::too_many_arguments)]
fn do_generic_backup(
    database: &str,
    app_container: &str,
    db_container: &str,
    host: &Host,
    db_port: &str,
    db_user: &str,
    db_password: &str,
    output_dir: &str,
    cron: bool,
) -> Result<()> {
    let ts = local_timestamp()?;
    let prefix = if cron { "cron" } else { "manual" };
    let fname = format!("{prefix}_{database}_{ts}");
    let tmp = "/tmp";
    let clean_output_dir = resolve_remote_path(output_dir);
    let remote_dest = format!("{clean_output_dir}/{fname}.tar.gz");
    let sql_tmp = format!("{tmp}/{fname}.sql");
    let fs_tar = format!("{tmp}/{fname}_fs.tar.gz");
    let final_tar = format!("{fname}.tar.gz");

    let result = run_backup_steps(
        &fname,
        tmp,
        database,
        app_container,
        db_container,
        host,
        db_port,
        db_user,
        db_password,
        &clean_output_dir,
        &remote_dest,
        &sql_tmp,
        &fs_tar,
        &final_tar,
    );

    if let Err(e) = &result {
        println!("❌ Erreur attrapée : {e}");
        println!("⚠️ Tentative de nettoyage sécurisée...");
        let _ = exec_remote(&["exec", db_container, "rm", "-f", &sql_tmp], host);
        let _ = exec_remote(
            &["exec", "-u", "root", app_container, "rm", "-f", &sql_tmp, &fs_tar, &format!("{tmp}/{final_tar}")],
            host,
        );
        let _ = exec_remote_shell(&format!("rm -f '{sql_tmp}'"), host);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn run_backup_steps(
    fname: &str,
    tmp: &str,
    database: &str,
    app_container: &str,
    db_container: &str,
    host: &Host,
    db_port: &str,
    db_user: &str,
    db_password: &str,
    clean_output_dir: &str,
    remote_dest: &str,
    sql_tmp: &str,
    fs_tar: &str,
    final_tar: &str,
) -> Result<()> {
    println!("📁 Création du dossier distant si nécessaire ({clean_output_dir})...");
    exec_remote_shell_checked(
        &format!("mkdir -p '{clean_output_dir}'"),
        host,
        "création du dossier distant",
    )?;

    println!("🗄️ Dump de la base de données (depuis {db_container})...");
    exec_remote_checked(
        &[
            "exec",
            "-e",
            &format!("PGPASSWORD={db_password}"),
            db_container,
            "pg_dump",
            "-h",
            "localhost",
            "-p",
            db_port,
            "-U",
            db_user,
            "-d",
            database,
            "-f",
            sql_tmp,
        ],
        host,
        "dump SQL (pg_dump)",
    )?;

    println!("🔄 Centralisation du fichier SQL vers le conteneur applicatif...");
    exec_remote_checked(
        &["cp", &format!("{db_container}:{sql_tmp}"), sql_tmp],
        host,
        "copie SQL conteneur DB -> hôte",
    )?;
    exec_remote_checked(
        &["cp", sql_tmp, &format!("{app_container}:{sql_tmp}")],
        host,
        "copie SQL hôte -> conteneur APP",
    )?;

    let _ = exec_remote(&["exec", db_container, "rm", "-f", sql_tmp], host);
    let _ = exec_remote_shell(&format!("rm -f '{sql_tmp}'"), host);

    println!("📂 Vérification du filestore (depuis {app_container})...");
    let fs_check_cmd = format!("[ -d '/var/lib/odoo/filestore/{database}' ] && echo ok");
    let fs_check = exec_remote(&["exec", app_container, "sh", "-c", &fs_check_cmd], host)?;

    if stdout_str(&fs_check) == "ok" {
        println!("📦 Compression du filestore...");
        let tar_fs_cmd = format!("cd /var/lib/odoo/filestore && tar -czf '{fs_tar}' '{database}'");
        exec_remote_checked(
            &["exec", app_container, "sh", "-c", &tar_fs_cmd],
            host,
            "compression du filestore",
        )?;
    } else {
        println!("⚠️ Filestore absent dans l'application, création d'une archive vide...");
        let empty_cmd = format!("mkdir -p {tmp}/empty && tar -czf {fs_tar} -C {tmp}/empty .");
        exec_remote_checked(
            &["exec", app_container, "sh", "-c", &empty_cmd],
            host,
            "archive filestore vide",
        )?;
    }

    println!("📦 Création de l'archive globale...");
    let tar_all_cmd = format!("cd '{tmp}' && tar -czf '{final_tar}' '{fname}.sql' '{fname}_fs.tar.gz'");
    exec_remote_checked(&["exec", app_container, "sh", "-c", &tar_all_cmd], host, "archive globale")?;

    println!("💾 Extraction vers le stockage du serveur [{remote_dest}]...");
    exec_remote_checked(
        &["cp", &format!("{app_container}:{tmp}/{final_tar}"), remote_dest],
        host,
        "extraction finale vers l'hôte",
    )?;

    println!("🧹 Nettoyage des fichiers temporaires...");
    let _ = exec_remote(
        &["exec", "-u", "root", app_container, "rm", "-f", sql_tmp, fs_tar, &format!("{tmp}/{final_tar}")],
        host,
    );

    println!("🎉 Succès ! Backup complet disponible sur le serveur : {remote_dest}");
    Ok(())
}

/// `backup restore` — restaure une archive dans le déploiement courant. **Destructif**
/// (`DROP DATABASE` sur la cible) : demande confirmation sauf si `force`.
pub fn cmd_backup_restore(
    backup_file: Option<String>,
    target_database: Option<String>,
    force: bool,
) -> Result<()> {
    let (customer_name, _) =
        customer::get_current_customer()?.ok_or_else(|| anyhow!("❌ Aucun client sélectionné."))?;
    let customers = config::load_customers()?;
    let customer_data = customers
        .get(&customer_name)
        .ok_or_else(|| anyhow!("Client '{customer_name}' introuvable"))?;

    let dep = deployment::get_current_deployment_info()?;
    let host_id = dep
        .hosts
        .first()
        .map(|h| h.host_id.clone())
        .ok_or_else(|| anyhow!("Déploiement sans hôte."))?;

    let hosts = config::load_hosts()?;
    let host = hosts
        .get(&host_id)
        .ok_or_else(|| anyhow!("Hôte '{host_id}' introuvable dans hosts.yaml"))?;

    let app_container = dep
        .container_name
        .clone()
        .ok_or_else(|| anyhow!("❌ Ce déploiement n'a pas de container_name configuré."))?;
    let db_container = dep
        .db_container_name
        .clone()
        .ok_or_else(|| anyhow!("❌ Ce déploiement n'a pas de db_container_name configuré."))?;

    println!("📦 Conteneur identifié : {app_container}");

    let creds = dep.db_credentials.as_ref().ok_or_else(|| {
        anyhow!("❌ Credentials DB manquants. Ajoutez 'db_credentials' dans customers.yaml.")
    })?;
    let db_password = secrets::reveal(&creds.password)
        .context("déchiffrement du mot de passe DB")?;

    let target_database = match target_database.filter(|d| !d.is_empty()) {
        Some(d) => d,
        None => dep.database_name.clone().ok_or_else(|| {
            anyhow!(
                "❌ Aucune base de données cible. Précisez --target-database ou configurez database_name pour ce déploiement."
            )
        })?,
    };

    let abbrev = customer_data.abbreviation.clone();

    let backup_file = match backup_file.filter(|f| !f.is_empty()) {
        Some(f) => f,
        None => {
            if abbrev.is_empty() {
                bail!("Abréviation client manquante, impossible de lister les backups.");
            }
            let backup_dir = resolve_remote_path(&format!("~/backups/{abbrev}/{host_id}"));
            println!("🔎 Recherche des backups disponibles ({backup_dir})...");
            let available = list_remote_backups(&backup_dir, host)?;
            if available.is_empty() {
                bail!("❌ Aucun backup trouvé dans {backup_dir}.");
            }
            match ui::select("Backup à restaurer :", available) {
                Some(s) => s,
                None => {
                    println!("❌ Restauration annulée.");
                    return Ok(());
                }
            }
        }
    };

    // Un backup peut venir d'un tout autre client/déploiement (restauration croisée) :
    // un chemin absolu est utilisé tel quel, sinon on le cherche dans le dossier de
    // backup habituel du client courant.
    let backup_path = if backup_file.starts_with('/') || backup_file.starts_with('~') {
        resolve_remote_path(&backup_file)
    } else {
        if abbrev.is_empty() {
            bail!("Abréviation client manquante et chemin de backup non-absolu fourni.");
        }
        resolve_remote_path(&format!("~/backups/{abbrev}/{host_id}/{backup_file}"))
    };

    println!("🔄 RESTAURATION ODOO");
    println!("📋 Client          : {customer_name}");
    println!("📋 Base cible      : {target_database}");
    println!("📋 Backup          : {backup_path}");

    if !force {
        println!(
            "⚠️ Ceci va DÉTRUIRE le contenu actuel de la base '{target_database}' sur {app_container}."
        );
        if !ui::confirm("Continuer ?") {
            println!("❌ Restauration annulée.");
            return Ok(());
        }
    }

    do_generic_restore(
        &target_database,
        &app_container,
        &db_container,
        host,
        &creds.port,
        &creds.user,
        &db_password,
        &backup_path,
    )
}

/// Moteur interne : arrête l'app, extrait l'archive, DROP+CREATE la base cible, restaure
/// le dump SQL puis le filestore (via `docker cp` pendant que l'app est arrêtée, `docker
/// exec` étant indisponible sur un conteneur stoppé), redémarre l'app, `chown` final.
/// Toute erreur déclenche une tentative de redémarrage de l'app + nettoyage best-effort
/// avant d'être propagée (comme le `try`/`catch` du nu).
#[allow(clippy::too_many_arguments)]
fn do_generic_restore(
    target_database: &str,
    app_container: &str,
    db_container: &str,
    host: &Host,
    db_port: &str,
    db_user: &str,
    db_password: &str,
    backup_path: &str,
) -> Result<()> {
    let tmp = "/tmp";
    let ts = local_timestamp()?;
    let work_dir = format!("{tmp}/restore_{ts}");

    let result = run_restore_steps(
        target_database,
        app_container,
        db_container,
        host,
        db_port,
        db_user,
        db_password,
        backup_path,
        tmp,
        &ts,
        &work_dir,
    );

    if let Err(e) = &result {
        println!("❌ Erreur attrapée : {e}");
        println!("⚠️ Tentative de redémarrage du conteneur applicatif...");
        let _ = exec_remote(&["start", app_container], host);
        let _ = exec_remote_shell(&format!("rm -rf '{work_dir}'"), host);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn run_restore_steps(
    target_database: &str,
    app_container: &str,
    db_container: &str,
    host: &Host,
    db_port: &str,
    db_user: &str,
    db_password: &str,
    backup_path: &str,
    tmp: &str,
    ts: &str,
    work_dir: &str,
) -> Result<()> {
    println!("🔎 Vérification du fichier de backup sur l'hôte...");
    exec_remote_shell_checked(&format!("test -f '{backup_path}'"), host, "vérification du fichier de backup")?;

    println!("🛑 Arrêt du conteneur applicatif ({app_container})...");
    exec_remote_checked(&["stop", app_container], host, "arrêt du conteneur applicatif")?;

    println!("📦 Extraction de l'archive sur l'hôte...");
    exec_remote_shell_checked(
        &format!("mkdir -p '{work_dir}' && tar -xzf '{backup_path}' -C '{work_dir}'"),
        host,
        "extraction de l'archive",
    )?;

    let sql_find = exec_remote_shell(&format!("ls {work_dir}/*.sql 2>/dev/null | head -1"), host)?;
    let sql_file = stdout_str(&sql_find);
    if sql_file.is_empty() {
        bail!("Aucun fichier .sql trouvé dans l'archive de backup.");
    }

    let fs_find = exec_remote_shell(&format!("ls {work_dir}/*_fs.tar.gz 2>/dev/null | head -1"), host)?;
    let fs_archive = stdout_str(&fs_find);

    println!("🗑️ Suppression de la base '{target_database}' si elle existe...");
    exec_remote_checked(
        &[
            "exec",
            "-e",
            &format!("PGPASSWORD={db_password}"),
            db_container,
            "psql",
            "-h",
            "localhost",
            "-p",
            db_port,
            "-U",
            db_user,
            "-d",
            "postgres",
            "-c",
            &format!("DROP DATABASE IF EXISTS \"{target_database}\""),
        ],
        host,
        "suppression de la base existante",
    )?;

    println!("🆕 Création de la base '{target_database}'...");
    exec_remote_checked(
        &[
            "exec",
            "-e",
            &format!("PGPASSWORD={db_password}"),
            db_container,
            "psql",
            "-h",
            "localhost",
            "-p",
            db_port,
            "-U",
            db_user,
            "-d",
            "postgres",
            "-c",
            &format!("CREATE DATABASE \"{target_database}\" OWNER \"{db_user}\" ENCODING 'UTF8'"),
        ],
        host,
        "création de la base cible",
    )?;

    println!("💾 Copie et restauration du dump SQL...");
    let restore_sql_tmp = format!("{tmp}/restore_{ts}.sql");
    exec_remote_checked(
        &["cp", &sql_file, &format!("{db_container}:{restore_sql_tmp}")],
        host,
        "copie du dump vers le conteneur DB",
    )?;
    exec_remote_checked(
        &[
            "exec",
            "-e",
            &format!("PGPASSWORD={db_password}"),
            db_container,
            "psql",
            "-h",
            "localhost",
            "-p",
            db_port,
            "-U",
            db_user,
            "-d",
            target_database,
            "-f",
            &restore_sql_tmp,
        ],
        host,
        "restauration du dump SQL",
    )?;
    let _ = exec_remote(&["exec", db_container, "rm", "-f", &restore_sql_tmp], host);

    let has_filestore = !fs_archive.is_empty();
    if has_filestore {
        println!("📂 Restauration du filestore...");
        let fs_extract_dir = format!("{work_dir}/fs_extract");
        exec_remote_shell_checked(
            &format!("mkdir -p '{fs_extract_dir}' && tar -xzf '{fs_archive}' -C '{fs_extract_dir}'"),
            host,
            "extraction du filestore",
        )?;

        let src_dir_result = exec_remote_shell(&format!("ls '{fs_extract_dir}'"), host)?;
        let src_dir_name = stdout_str(&src_dir_result);

        if src_dir_name.is_empty() {
            println!("⚠️ Archive de filestore vide, rien à restaurer.");
        } else {
            // Le suffixe '/.' copie le CONTENU du répertoire source, pas le répertoire
            // lui-même — l'archive contient un répertoire nommé d'après la base D'ORIGINE.
            exec_remote_checked(
                &[
                    "cp",
                    &format!("{fs_extract_dir}/{src_dir_name}/."),
                    &format!("{app_container}:/var/lib/odoo/filestore/{target_database}"),
                ],
                host,
                "copie du filestore vers le conteneur APP",
            )?;
        }
    } else {
        println!("⚠️ Aucun filestore dans l'archive, restauration SQL uniquement.");
    }

    println!("🧹 Nettoyage des fichiers temporaires sur l'hôte...");
    let _ = exec_remote_shell(&format!("rm -rf '{work_dir}'"), host);

    println!("🚀 Redémarrage du conteneur applicatif ({app_container})...");
    exec_remote_checked(&["start", app_container], host, "redémarrage du conteneur applicatif")?;

    if has_filestore {
        let _ = exec_remote(
            &[
                "exec",
                "-u",
                "root",
                app_container,
                "chown",
                "-R",
                "odoo:odoo",
                &format!("/var/lib/odoo/filestore/{target_database}"),
            ],
            host,
        );
    }

    println!("🎉 Succès ! Base '{target_database}' restaurée depuis {backup_path}");
    Ok(())
}

#[cfg(test)]
mod tests;
