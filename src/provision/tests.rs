//! Tests de `push_file`/`push_binary`. `sha256_hex` reste réel et non-mocké (voir
//! PORTING.md) : c'est le côté SSH (mkdir/rm/echo|base64/sha256sum/chmod) qui est
//! intercepté via le seam `ssh::spawn`, pas le calcul de hachage local lui-même.

use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use super::*;

fn localhost_host() -> Host {
    Host {
        name: "localhost".to_string(),
        hostname: "localhost".to_string(),
        user: String::new(),
        port: String::new(),
        identity_file: String::new(),
        arch: String::new(),
        docker_context: String::new(),
        identity_key: None,
    }
}

/// Hôte fictif, jamais réel (même précaution que `ssh/tests.rs`/`docker/tests.rs`) :
/// un hostname/user/port réels pourrait coïncider avec un vrai socket `controlmasters/`
/// déjà ouvert. `tag` distingue le socket factice d'un test à l'autre — `cargo test`
/// exécute les tests en parallèle sur des threads différents, et deux tests partageant
/// le même chemin de socket factice se marchent dessus (l'un peut le supprimer au
/// `Drop` pendant que l'autre le croit encore présent).
fn remote_host(tag: &str) -> Host {
    Host {
        name: format!("test-unreachable-provision-{tag}"),
        hostname: "test-host-unreachable.invalid".to_string(),
        user: format!("test-user-seam-provision-{tag}"),
        port: "1".to_string(),
        identity_file: String::new(),
        arch: String::new(),
        docker_context: String::new(),
        identity_key: None,
    }
}

fn scratch_path(name: &str) -> String {
    std::env::temp_dir().join(format!("ppo-provision-test-{name}-{}", std::process::id())).display().to_string()
}

#[test]
fn push_file_local_ecrit_le_contenu_tel_quel() {
    let host = localhost_host();
    let path = scratch_path("push-file-local");
    push_file(&host, &path, "contenu de test\n").unwrap();

    let written = std::fs::read_to_string(&path).unwrap();
    assert_eq!(written, "contenu de test\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn push_file_distant_encode_en_base64_et_cree_le_dossier_parent() {
    let host = remote_host("push-file");
    let _socket = ssh::with_fake_socket(&host);
    let commands: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let commands2 = commands.clone();
    let _g = ssh::install_connected_test_runner(move |program, args| {
        assert_eq!(program, "ssh");
        commands2.lock().unwrap().push(args.last().unwrap().clone());
        Ok(ssh::fake_output(0, "", ""))
    });

    push_file(&host, "/srv/app/config.yml", "clé: valeur\n").unwrap();

    let sent = commands.lock().unwrap();
    assert!(sent[0].starts_with("mkdir -p '/srv/app'"), "dossier parent attendu : {sent:?}");
    let send_cmd = sent.iter().find(|c| c.contains("base64 -d >")).expect("envoi base64 attendu");
    let encoded = send_cmd
        .strip_prefix("echo '")
        .and_then(|s| s.split("' |").next())
        .expect("format 'echo <b64> | base64 -d > path' attendu");
    let decoded = BASE64.decode(encoded).unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), "clé: valeur\n");
}

#[test]
fn push_binary_local_ecrit_et_rend_executable() {
    let host = localhost_host();
    let path = scratch_path("push-binary-local");
    push_binary(&host, &path, b"faux-binaire").unwrap();

    let written = std::fs::read(&path).unwrap();
    assert_eq!(written, b"faux-binaire");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn push_binary_distant_echoue_et_saute_chmod_si_le_hash_ne_correspond_pas() {
    let host = remote_host("push-binary-mismatch");
    let _socket = ssh::with_fake_socket(&host);
    let chmod_called = Arc::new(Mutex::new(false));
    let chmod_called2 = chmod_called.clone();
    let _g = ssh::install_connected_test_runner(move |program, args| {
        assert_eq!(program, "ssh");
        let cmd = args.last().unwrap().clone();
        if cmd.starts_with("sha256sum ") {
            return Ok(ssh::fake_output(0, "deadbeef  /srv/app/agent\n", ""));
        }
        if cmd.starts_with("chmod +x") {
            *chmod_called2.lock().unwrap() = true;
        }
        Ok(ssh::fake_output(0, "", ""))
    });

    let err = push_binary(&host, "/srv/app/agent", b"contenu-binaire").unwrap_err();
    assert!(err.to_string().contains("Intégrité du transfert"));
    assert!(!*chmod_called.lock().unwrap(), "chmod +x ne doit pas être appelé sur un hash invalide");
}

#[test]
fn push_binary_distant_reussit_et_chmod_le_fichier_sur_hash_correct() {
    let host = remote_host("push-binary-match");
    let _socket = ssh::with_fake_socket(&host);
    let bytes = b"contenu-binaire";
    let expected_hash = sha256_hex(bytes).unwrap();
    let chmod_called = Arc::new(Mutex::new(false));
    let chmod_called2 = chmod_called.clone();
    let _g = ssh::install_connected_test_runner(move |program, args| {
        assert_eq!(program, "ssh");
        let cmd = args.last().unwrap().clone();
        if cmd.starts_with("sha256sum ") {
            return Ok(ssh::fake_output(0, &format!("{expected_hash}  /srv/app/agent\n"), ""));
        }
        if cmd.starts_with("chmod +x") {
            *chmod_called2.lock().unwrap() = true;
        }
        Ok(ssh::fake_output(0, "", ""))
    });

    push_binary(&host, "/srv/app/agent", bytes).unwrap();
    assert!(*chmod_called.lock().unwrap(), "chmod +x attendu sur un hash correct");
}
