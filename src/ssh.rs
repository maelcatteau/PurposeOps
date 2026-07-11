//! SSH ControlMaster (port de `ssh-manager.nu`).
//!
//! `is_master_active`/`create_master_connection`/`run_with_master` sont des fonctions
//! internes (comme côté nu : aucune commande CLI publique n'appelle directement le SSH
//! brut, seuls le Docker (Phase 4) et le backup (Phase 6) le feront). Les sockets vivent
//! dans le même dossier `controlmasters/` que le module nu, avec le même nommage
//! `user@hostname:port` — ce qui permet aux deux outils de **réutiliser la même
//! connexion multiplexée** pendant la coexistence.

// Temporaire : is_master_active/create_master_connection/run_with_master ne seront
// appelées par du code CLI qu'en Phase 4 (Docker) et 6 (backup) ; ici elles sont déjà
// couvertes par le test live #[ignore]. À retirer quand Docker les consomme.
#![allow(dead_code)]

use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, bail};

use crate::config::Host;
use crate::secrets;

#[cfg(test)]
type Runner = dyn Fn(&str, &[String]) -> std::io::Result<Output>;

#[cfg(test)]
thread_local! {
    static TEST_RUNNER: std::cell::RefCell<Option<Box<Runner>>> =
        const { std::cell::RefCell::new(None) };
}

/// Point de passage unique pour toute commande externe lancée par ce module (et par
/// `docker.rs`, via `spawn` réutilisée telle quelle pour son branchement `localhost`) —
/// voir PORTING.md pour pourquoi ce plafond existe (limite Linux `MAX_ARG_STRLEN` sur un
/// autre sujet, mais même famille de découverte : les points de spawn dispersés sont
/// difficiles à faire évoluer ensemble). En build normal, exactement
/// `Command::new(program).args(args).output()` — aucun changement de comportement/perf.
/// En build de test, si un faux exécuteur a été installé sur ce thread
/// (`install_test_runner`), il est appelé à la place, sans toucher à un vrai processus.
pub(crate) fn spawn(program: &str, args: &[String]) -> std::io::Result<Output> {
    #[cfg(test)]
    {
        let intercepted = TEST_RUNNER.with(|r| r.borrow().as_ref().map(|f| f(program, args)));
        if let Some(result) = intercepted {
            return result;
        }
    }
    Command::new(program).args(args).output()
}

#[cfg(test)]
pub(crate) struct TestRunnerGuard;

#[cfg(test)]
impl Drop for TestRunnerGuard {
    fn drop(&mut self) {
        TEST_RUNNER.with(|r| *r.borrow_mut() = None);
    }
}

/// Installe `f` comme faux exécuteur de processus pour le thread de test courant,
/// et retourne un guard qui le retire au `drop`. Important : les threads ouvriers de
/// `cargo test` sont réutilisés d'un test à l'autre — sans ce nettoyage automatique, un
/// faux exécuteur oublié fuirait dans le test suivant programmé sur le même thread.
#[cfg(test)]
pub(crate) fn install_test_runner<F>(f: F) -> TestRunnerGuard
where
    F: Fn(&str, &[String]) -> std::io::Result<Output> + 'static,
{
    TEST_RUNNER.with(|r| *r.borrow_mut() = Some(Box::new(f)));
    TestRunnerGuard
}

/// Comme `install_test_runner`, mais répond d'avance "actif" à la sonde de vivacité
/// `ssh -O check` — pratique pour un test qui veut vérifier la commande réellement
/// envoyée sans avoir à simuler toute la mise en place de la connexion ControlMaster.
#[cfg(test)]
pub(crate) fn install_connected_test_runner<F>(handler: F) -> TestRunnerGuard
where
    F: Fn(&str, &[String]) -> std::io::Result<Output> + 'static,
{
    install_test_runner(move |program, args| {
        if program == "ssh" && args.iter().any(|a| a == "check") {
            return Ok(fake_output(0, "", ""));
        }
        handler(program, args)
    })
}

/// `Output` de complaisance partagé par les tests de tout le crate (remplace la copie
/// ad hoc jusqu'ici dupliquée dans `backup/tests.rs`).
#[cfg(test)]
pub(crate) fn fake_output(code: i32, stdout: &str, stderr: &str) -> Output {
    use std::os::unix::process::ExitStatusExt;
    Output {
        status: std::process::ExitStatus::from_raw(code),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

/// `is_master_active` retourne `false` sans jamais appeler `spawn` si le fichier de
/// socket n'existe pas sur le vrai disque (vérification filesystem, délibérément hors
/// du seam process-exec — voir PORTING.md). Pour exercer le chemin "connexion déjà
/// active" malgré ça, dans n'importe quel module de test du crate, on crée un fichier
/// de socket factice (vide, inoffensif) au chemin exact que `control_socket`
/// calculerait — retiré au `drop`, même si le test panique en cours de route.
#[cfg(test)]
pub(crate) struct FakeSocket(PathBuf);

#[cfg(test)]
impl Drop for FakeSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
pub(crate) fn with_fake_socket(host: &Host) -> FakeSocket {
    let path = control_socket(host);
    std::fs::write(&path, b"").expect("écriture du socket factice");
    FakeSocket(path)
}

fn control_path() -> PathBuf {
    let home = std::env::var("HOME").expect("$HOME non défini");
    let dir = PathBuf::from(home).join("dev/nu-modules/PurposeOps/controlmasters");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).expect("création de controlmasters/");
    }
    dir
}

fn control_socket(host: &Host) -> PathBuf {
    control_path().join(format!("{}@{}:{}", host.user, host.hostname, host.port))
}

fn ssh_target(host: &Host) -> String {
    format!("{}@{}", host.user, host.hostname)
}

/// `~/...` → `$HOME/...` ; `./...` → chemin absolu. Sinon inchangé.
pub fn resolve_key_path(identity_file: &str) -> String {
    if let Some(rest) = identity_file.strip_prefix("~/") {
        let home = std::env::var("HOME").expect("$HOME non défini");
        format!("{home}/{rest}")
    } else if identity_file.starts_with("./") {
        std::fs::canonicalize(identity_file)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| identity_file.to_string())
    } else {
        identity_file.to_string()
    }
}

/// Le nu remplace en dur `~` par le home de l'utilisateur SSH DISTANT (pas celui du
/// laptop, à l'inverse de `resolve_key_path` ci-dessus) : un `~/...` entre quotes simples
/// dans une commande shell distante n'est de toute façon jamais tilde-expansé par le
/// shell — voir la note dans CLAUDE.md. Dérivé de `user` (`/home/<user>`) plutôt que codé
/// en dur : ça a longtemps été correct en dur (`/home/ngner`) uniquement parce que
/// l'utilisateur SSH de chaque hôte du parc était justement `ngner` partout — un vrai bug
/// trouvé en écrivant `tests/backup_agent_workflow.py` contre la VM de test (utilisateur
/// `ppo`, pas `ngner`) a confirmé que l'hypothèse ne tenait pas dès qu'un hôte a un
/// utilisateur différent. Partagée entre `backup.rs` et `backup_agent.rs`.
pub(crate) fn remote_home(user: &str) -> String {
    format!("/home/{user}")
}

pub(crate) fn resolve_remote_path(path: &str, user: &str) -> String {
    path.replace('~', &remote_home(user))
}

fn key_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("$HOME non défini");
    PathBuf::from(home).join(".cache/ppo/keys")
}

/// Déchiffre `host.identity_key` et l'écrit dans un fichier `0600` en cache local
/// (recréé à chaque appel — le déchiffrement `age` est de l'ordre de la microseconde,
/// pas la peine de gérer un cache invalidable). Un nom de fichier stable par hôte évite
/// l'accumulation de fichiers temporaires.
fn materialize_identity_key(host: &Host, encrypted: &str) -> Result<String> {
    let plaintext = secrets::reveal(encrypted)?;

    let dir = key_cache_dir();
    std::fs::create_dir_all(&dir)?;
    let sanitized: String = host
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let path = dir.join(sanitized);

    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file = opts.open(&path)?;
    file.write_all(plaintext.as_bytes())?;

    Ok(path.display().to_string())
}

/// Chemin de la clé SSH à utiliser pour cet hôte : `identity_key` (déchiffré et
/// matérialisé) si présent et déchiffrable localement, sinon repli sur `identity_file`.
/// Un échec de déchiffrement de `identity_key` n'est qu'un avertissement — il ne doit
/// jamais bloquer une commande SSH qui aurait pu passer via `identity_file`.
fn resolved_identity_path(host: &Host) -> Option<String> {
    if let Some(encrypted) = &host.identity_key {
        match materialize_identity_key(host, encrypted) {
            Ok(path) => return Some(path),
            Err(e) => {
                eprintln!(
                    "⚠️  identity_key illisible pour '{}' ({e}), repli sur identity_file",
                    host.name
                );
            }
        }
    }
    if host.identity_file.is_empty() {
        None
    } else {
        Some(resolve_key_path(&host.identity_file))
    }
}

/// Options communes `-S socket -p port -o ... [-i key]`, sans la cible.
fn common_args(socket: &std::path::Path, host: &Host) -> Vec<String> {
    let mut args = vec![
        "-S".to_string(),
        socket.display().to_string(),
        "-p".to_string(),
        host.port.clone(),
        "-o".to_string(),
        "StrictHostKeyChecking=no".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    if let Some(path) = resolved_identity_path(host) {
        args.push("-i".to_string());
        args.push(path);
    }
    args
}

/// Le socket existe-t-il et la connexion est-elle réellement active (`ssh -O check`) ?
pub fn is_master_active(host: &Host) -> bool {
    let socket = control_socket(host);
    if !socket.exists() {
        return false;
    }
    let args = vec![
        "-O".to_string(),
        "check".to_string(),
        "-S".to_string(),
        socket.display().to_string(),
        ssh_target(host),
    ];
    spawn("ssh", &args).map(|o| o.status.success()).unwrap_or(false)
}

/// Crée la connexion maître (`ssh -M -N -f -n ...`). Nettoie un socket orphelin
/// avant de (re)tenter. Une erreur de `ssh -M` peut être bénigne (le vrai verdict
/// est `is_master_active` juste après), donc on ne remonte pas l'erreur ici.
pub fn create_master_connection(host: &Host) -> bool {
    let socket = control_socket(host);
    let target = ssh_target(host);
    println!("🔄 Creating master connection to {target}...");

    if socket.exists() && !is_master_active(host) {
        println!("🧹 Nettoyage d'un socket orphelin...");
        let _ = std::fs::remove_file(&socket);
    }

    let mut args = vec!["-M".to_string(), "-N".to_string(), "-f".to_string(), "-n".to_string()];
    args.extend(common_args(&socket, host));
    args.push(target.clone());

    if let Err(e) = spawn("ssh", &args) {
        println!("⚠️ ssh -M a retourné une erreur (potentiellement bénigne) : {e}");
    }

    sleep(Duration::from_millis(500));

    if is_master_active(host) {
        println!("✅ Master connection established.");
        true
    } else {
        println!("❌ Échec création master : le socket existe mais est inactif.");
        if socket.exists() {
            let _ = std::fs::remove_file(&socket);
        }
        false
    }
}

/// Exécute `command` sur l'hôte distant via la connexion maître (créée si besoin).
/// `command` est passé comme un seul argument à `ssh`, qui le transmet tel quel au
/// shell distant — pas de découpage/quoting local, contrairement à un `sh -c` construit
/// par interpolation.
pub fn run_with_master(host: &Host, command: &str) -> Result<Output> {
    if !is_master_active(host) && !create_master_connection(host) {
        bail!("Failed to establish master connection");
    }

    let socket = control_socket(host);
    // Parité avec le nu : échappement des accolades doubles avant transmission.
    let escaped = command.replace("{{", "\\{\\{").replace("}}", "\\}\\}");

    let mut args = common_args(&socket, host);
    args.push(ssh_target(host));
    args.push(escaped);

    Ok(spawn("ssh", &args)?)
}

/// Exécute une commande shell brute sur l'hôte : localement si `hostname == "localhost"`,
/// sinon via la connexion ControlMaster. Contrairement à `run_with_master`, gère aussi le
/// cas local — c'est la primitive commune à `provision` et `bootstrap` pour toute commande
/// qui n'est pas du `docker` (voir `docker::run_docker_command` pour ce cas-là).
pub fn exec_shell(host: &Host, cmd: &str) -> Result<Output> {
    if host.hostname == "localhost" {
        Ok(spawn("sh", &["-c".to_string(), cmd.to_string()])?)
    } else {
        run_with_master(host, cmd)
    }
}

/// Comme `exec_shell`, mais transforme un code de sortie non nul en erreur (stderr inclus).
/// `step` identifie l'étape dans le message d'erreur.
pub fn exec_shell_checked(host: &Host, cmd: &str, step: &str) -> Result<Output> {
    let output = exec_shell(host, cmd)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Échec de l'étape '{step}' : {}", stderr.trim());
    }
    Ok(output)
}

/// Ferme la connexion maître d'un hôte donné. `true` si fermée ou déjà absente.
pub fn close_master_connection(host: &Host) -> bool {
    let socket = control_socket(host);
    let target = ssh_target(host);

    if !socket.exists() {
        println!("ℹ️  No master connection exists for {target}");
        return true;
    }
    if !is_master_active(host) {
        println!("ℹ️  Master connection for {target} is already inactive");
        let _ = std::fs::remove_file(&socket);
        return true;
    }

    println!("🔄 Closing master connection to {target}...");
    let args = vec![
        "-O".to_string(),
        "exit".to_string(),
        "-S".to_string(),
        socket.display().to_string(),
        target.clone(),
    ];
    let result = spawn("ssh", &args).map(|o| o.status.success()).unwrap_or(false);

    if result {
        println!("✅ Master connection closed for {target}");
    } else {
        println!("❌ Failed to close master connection");
    }
    if socket.exists() {
        let _ = std::fs::remove_file(&socket);
    }
    result
}

/// `user@hostname:port` → (user, hostname, port). `None` si le nom ne matche pas.
fn parse_socket_name(name: &str) -> Option<(String, String, String)> {
    let (user, rest) = name.split_once('@')?;
    let (hostname, port) = rest.rsplit_once(':')?;
    Some((user.to_string(), hostname.to_string(), port.to_string()))
}

/// Reconstruit un `Host` minimal à partir d'un nom de socket (pas d'identity_file :
/// on n'en a pas besoin pour `-O check`/`-O exit`, qui n'authentifient pas).
fn host_from_socket_name(name: &str) -> Option<Host> {
    let (user, hostname, port) = parse_socket_name(name)?;
    Some(Host {
        name: hostname.clone(),
        hostname,
        user,
        port,
        identity_file: String::new(),
        arch: String::new(),
        docker_context: String::new(),
        identity_key: None,
    })
}

fn list_sockets() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(control_path()) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

/// `closeall` — ferme toutes les connexions maîtres actives.
pub fn close_all_master_connections() {
    println!("🔄 Closing all master connections...");
    let sockets = list_sockets();
    if sockets.is_empty() {
        println!("ℹ️  No master connections found");
        return;
    }

    let mut closed = 0;
    for name in &sockets {
        println!("🔄 Processing {name}...");
        let Some(host) = host_from_socket_name(name) else {
            println!("  ⚠️  Failed to parse {name}");
            continue;
        };
        let target = ssh_target(&host);
        let socket = control_socket(&host);
        let args = vec![
            "-O".to_string(),
            "exit".to_string(),
            "-S".to_string(),
            socket.display().to_string(),
            target.clone(),
        ];
        let ok = spawn("ssh", &args).map(|o| o.status.success()).unwrap_or(false);
        if ok {
            println!("  ✅ Closed connection to {target}");
            closed += 1;
        } else {
            println!("  ⚠️  Failed to close {name}");
        }
        let _ = std::fs::remove_file(&socket);
    }
    println!("✅ Closed {closed} master connections");
}

/// `lsconn` — liste les connexions maîtres avec leur statut.
pub fn list_master_connections() {
    println!("🔍 Active master connections:");
    let sockets = list_sockets();
    if sockets.is_empty() {
        println!("ℹ️  No master connections found");
        return;
    }
    for name in &sockets {
        match host_from_socket_name(name) {
            Some(host) => {
                let status = if is_master_active(&host) {
                    "🟢 ACTIVE"
                } else {
                    "🔴 INACTIVE"
                };
                println!("  {name} - {status}");
            }
            None => println!("  {name} - ❓ UNKNOWN FORMAT"),
        }
    }
}

/// `close` — ferme la connexion de l'hôte actuellement sélectionné dans le contexte.
pub fn close_current_master_connection() -> Result<()> {
    let ctx = crate::config::load_context()?;
    let Some((_, host)) = ctx.host.into_iter().next() else {
        println!("ℹ️  Aucun hôte sélectionné");
        return Ok(());
    };
    if host.hostname == "localhost" {
        println!("ℹ️  No master connection to close for localhost");
        return Ok(());
    }
    close_master_connection(&host);
    Ok(())
}

#[cfg(test)]
mod tests;
