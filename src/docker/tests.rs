//! Tests Docker. `shell_quote` est LE test qui vise la classe de bugs de quoting/
//! interpolation documentée dans CLAUDE.md (Gotchas) — chaque argument doit survivre
//! intact, quel que soit son contenu, une fois rejoint dans une commande shell distante.

use super::*;

#[test]
fn shell_quote_simple() {
    assert_eq!(shell_quote("hello"), "'hello'");
}

#[test]
fn shell_quote_espaces() {
    assert_eq!(shell_quote("hello world"), "'hello world'");
}

#[test]
fn shell_quote_quote_simple_interne() {
    // Cas historiquement piégeux : une quote simple à l'intérieur de l'argument.
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
}

#[test]
fn shell_quote_dollar_pas_interpole() {
    // Les guillemets simples empêchent l'expansion shell de $VAR et $(...).
    assert_eq!(shell_quote("$HOME"), "'$HOME'");
    assert_eq!(shell_quote("$(rm -rf /)"), "'$(rm -rf /)'");
}

#[test]
fn shell_quote_parentheses_litterales() {
    // Le gotcha CLAUDE.md : des parenthèses non échappées posent problème côté nu ;
    // ici elles sont juste des octets du mot quoté, aucun risque de substitution.
    assert_eq!(shell_quote("(depuis (x))"), "'(depuis (x))'");
}

#[test]
fn shell_quote_vide() {
    assert_eq!(shell_quote(""), "''");
}

#[test]
fn round_trip_via_argument_unique_reconstruit_le_mot_original() {
    // Preuve bout en bout : reconstruire "docker <args quotés> | sh" doit redonner
    // exactement les arguments d'origine, y compris ceux à espaces/quotes/`$`.
    let args = ["exec", "-e", "PGPASSWORD=it's a $ecret", "sh -c 'echo hi'"];
    let quoted: Vec<String> = args.iter().map(|a| shell_quote(a)).collect();
    let cmd_string = format!("docker {}", quoted.join(" "));

    // On demande à un vrai /bin/sh de découper la chaîne et de ré-émettre chaque
    // argument sur sa propre ligne (via printf), pour vérifier qu'aucun argument
    // n'a été fusionné/coupé par les espaces ou caractères spéciaux qu'il contient.
    let script = cmd_string.replacen("docker", "printf '%s\\n'", 1);
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .output()
        .expect("sh doit être disponible");
    let got = String::from_utf8_lossy(&output.stdout);
    let expected: Vec<&str> = args.to_vec();
    let actual: Vec<&str> = got.lines().collect();
    assert_eq!(actual, expected, "commande construite : {script}");
}

#[test]
fn parse_ndjson_plusieurs_lignes() {
    let input = b"{\"Names\":\"a\",\"Image\":\"i1\",\"Status\":\"Up\",\"Ports\":\"\"}\n{\"Names\":\"b\",\"Image\":\"i2\",\"Status\":\"Up\",\"Ports\":\"\"}\n";
    let entries: Vec<PsEntry> = parse_ndjson(input).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].names, "a");
    assert_eq!(entries[1].names, "b");
}

#[test]
fn parse_ndjson_ignore_les_lignes_vides() {
    let input = b"{\"Names\":\"a\",\"Image\":\"i\",\"Status\":\"Up\",\"Ports\":\"\"}\n\n\n";
    let entries: Vec<PsEntry> = parse_ndjson(input).unwrap();
    assert_eq!(entries.len(), 1);
}

#[test]
fn parse_ndjson_entree_vide_donne_liste_vide() {
    let entries: Vec<PsEntry> = parse_ndjson(b"").unwrap();
    assert!(entries.is_empty());
}

#[test]
fn parse_ndjson_ligne_invalide_est_une_erreur() {
    let result: Result<Vec<PsEntry>> = parse_ndjson(b"pas du json\n");
    assert!(result.is_err());
}

#[test]
fn regex_contains_aucun_filtre_matche_toujours() {
    assert!(regex_contains(&None, "n'importe quoi").unwrap());
}

#[test]
fn regex_contains_filtre_qui_matche() {
    let filter = Some("^odoo-.*".to_string());
    assert!(regex_contains(&filter, "odoo-prod").unwrap());
}

#[test]
fn regex_contains_filtre_qui_ne_matche_pas() {
    let filter = Some("^odoo-.*".to_string());
    assert!(!regex_contains(&filter, "vaultwarden").unwrap());
}

#[test]
fn regex_contains_regex_invalide_est_une_erreur() {
    let filter = Some("(".to_string());
    assert!(regex_contains(&filter, "peu importe").is_err());
}

#[test]
fn container_op_need_all_vrai_seulement_pour_start() {
    assert!(ContainerOp::Start.need_all());
    assert!(!ContainerOp::Stop.need_all());
    assert!(!ContainerOp::Restart.need_all());
}

#[test]
fn container_op_labels_distincts_par_variante() {
    let ops = [ContainerOp::Start, ContainerOp::Stop, ContainerOp::Restart];
    let headers: Vec<&str> = ops.iter().map(|o| o.header()).collect();
    let verbs: Vec<&str> = ops.iter().map(|o| o.verb()).collect();
    let participles: Vec<&str> = ops.iter().map(|o| o.past_participle()).collect();
    let commands: Vec<&str> = ops.iter().map(|o| o.docker_command()).collect();
    for labels in [&headers, &verbs, &participles, &commands] {
        let mut sorted = labels.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "attendu 3 libellés distincts, obtenu {labels:?}");
    }
}

#[test]
fn container_op_docker_command_correspond_au_sous_commande_docker_attendue() {
    assert_eq!(ContainerOp::Start.docker_command(), "start");
    assert_eq!(ContainerOp::Stop.docker_command(), "stop");
    assert_eq!(ContainerOp::Restart.docker_command(), "restart");
}

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

/// Hôte fictif, jamais réel (voir la même précaution dans `ssh/tests.rs`) : un
/// hostname/user/port réels pourrait coïncider avec un vrai socket `controlmasters/`
/// déjà ouvert et faire échouer l'écriture du socket factice avec ENXIO.
fn remote_host() -> Host {
    Host {
        name: "test-unreachable-docker".to_string(),
        hostname: "test-host-unreachable.invalid".to_string(),
        user: "test-user-seam-docker".to_string(),
        port: "1".to_string(),
        identity_file: String::new(),
        arch: String::new(),
        docker_context: String::new(),
        identity_key: None,
    }
}

#[test]
fn run_docker_command_en_local_appelle_docker_directement() {
    let host = localhost_host();
    let _g = ssh::install_test_runner(|program, args| {
        assert_eq!(program, "docker");
        assert_eq!(args, ["ps", "-a", "--format", "json"]);
        Ok(ssh::fake_output(0, "", ""))
    });

    let out = run_docker_command(&["ps", "-a", "--format", "json"], &host).unwrap();
    assert!(out.status.success());
}

#[test]
fn run_docker_command_a_distance_quote_les_arguments_dans_une_seule_commande_ssh() {
    let host = remote_host();
    let _socket = ssh::with_fake_socket(&host);
    let _g = ssh::install_connected_test_runner(|program, args| {
        assert_eq!(program, "ssh");
        let sent = args.last().unwrap();
        assert_eq!(sent, "docker 'inspect' 'my container'");
        Ok(ssh::fake_output(0, "", ""))
    });

    let out = run_docker_command(&["inspect", "my container"], &host).unwrap();
    assert!(out.status.success());
}
