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
