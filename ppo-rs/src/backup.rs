//! `backup run` (port de `customer-manager/backup.nu`'s `backup run` + `do-generic-backup`).
//!
//! Écarts volontaires avec le nu, purement du nettoyage — comportement inchangé :
//! - `--service` (jamais lu dans le corps de `backup run` côté nu) et `--silent` (jamais
//!   lu ni dans `backup run` ni dans `do-generic-backup`) sont des paramètres morts côté
//!   nu ; ils ne sont pas repris ici.
//! - `--dbHost` est accepté par `do-generic-backup` côté nu mais jamais utilisé dans la
//!   commande `pg_dump` (qui force `-h localhost`, cohérent puisque `pg_dump` tourne
//!   *dans* le conteneur DB) : le paramètre correspondant n'est pas non plus repris ici.
//! - Le bloc `🔍 DEBUG VARIABLES` de `backup run` (dump de variables internes) était du
//!   débogage laissé en place, pas un comportement voulu : pas porté.

use std::process::{Command, Output};

use anyhow::{Result, anyhow, bail};

use crate::config::{self, Host};
use crate::{customer, deployment, docker, ssh};

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
        &creds.password,
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

#[cfg(test)]
mod tests;
