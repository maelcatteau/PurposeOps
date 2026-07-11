use super::*;

#[test]
fn substitute_remplace_toutes_les_occurrences() {
    let template = "{{a}}-{{b}}-{{a}}";
    let mut vars = BTreeMap::new();
    vars.insert("a".to_string(), "X".to_string());
    vars.insert("b".to_string(), "Y".to_string());
    assert_eq!(substitute(template, &vars), "X-Y-X");
}

#[test]
fn substitute_laisse_les_placeholders_sans_valeur_correspondante() {
    let template = "{{known}}-{{unknown}}";
    let mut vars = BTreeMap::new();
    vars.insert("known".to_string(), "ok".to_string());
    assert_eq!(substitute(template, &vars), "ok-{{unknown}}");
}

#[test]
fn expand_networks_absent_est_un_no_op() {
    let mut vars = BTreeMap::new();
    vars.insert("other".to_string(), "value".to_string());
    expand_networks(&mut vars);
    assert_eq!(vars.len(), 1);
}

#[test]
fn expand_networks_un_seul_reseau() {
    let mut vars = BTreeMap::new();
    vars.insert("networks".to_string(), "caddy-network".to_string());
    expand_networks(&mut vars);
    assert_eq!(vars["networks_section"], "- caddy-network");
    assert_eq!(vars["networks_definition"], "caddy-network:\n    external: true");
}

#[test]
fn expand_networks_plusieurs_reseaux_avec_espaces() {
    let mut vars = BTreeMap::new();
    vars.insert(
        "networks".to_string(),
        "caddy-portainer, caddy-frontend,caddy-homarr".to_string(),
    );
    expand_networks(&mut vars);
    assert_eq!(
        vars["networks_section"],
        "- caddy-portainer\n      - caddy-frontend\n      - caddy-homarr"
    );
    assert_eq!(
        vars["networks_definition"],
        "caddy-portainer:\n    external: true\n  caddy-frontend:\n    external: true\n  caddy-homarr:\n    external: true"
    );
}

#[test]
fn format_example_string_sans_guillemets() {
    let v: serde_yaml_ng::Value = serde_yaml_ng::from_str("\"https://vault.customer.com\"").unwrap();
    assert_eq!(format_example(&v), "https://vault.customer.com");
}

#[test]
fn format_example_entier() {
    let v: serde_yaml_ng::Value = serde_yaml_ng::from_str("80").unwrap();
    assert_eq!(format_example(&v), "80");
}

/// Copie fidèle de `templates/Caddy/docker-compose.yml` (dépôt réel) — inlinée plutôt que
/// lue sur disque pour que le test ne dépende pas de l'arborescence locale, même
/// convention que `config/tests.rs`'s `FULL_CONTEXT`.
const CADDY_COMPOSE: &str = "services:
  {{service_name}}:
    image: caddy:2-alpine
    container_name: \"{{container_name}}\"
    restart: unless-stopped
    ports:
      - \"{{http_port}}:80\"     # HTTP
      - \"{{https_port}}:443\"   # HTTPS
      - \"{{admin_port}}:2019\"  # Admin
    networks:
      {{networks_section}}
    volumes:
      - \"{{config_volume_path}}:/etc/caddy\"
    environment:
      - CADDY_ADMIN={{admin_bind}}

networks:
  {{networks_definition}}
";

/// Copie fidèle de `templates/Vaultwarden/docker-compose.yml`.
const VAULTWARDEN_COMPOSE: &str = "services:
  {{service_name}}:
    image: vaultwarden/server:latest
    container_name: \"{{container_name}}\"
    restart: unless-stopped
    environment:
      DOMAIN: \"{{domain}}\"
    volumes:
      - \"{{data_volume_path}}:/data/\"
    expose:
      - \"{{exposed_port}}\"
    networks:
      - {{network}}

networks:
  {{network}}:
    external: true
";

/// Rend un template avec des variables déjà collectées, sans prompt interactif — même
/// pipeline que la fin de `generate_compose` (expand_networks puis substitute).
fn render(template: &str, docker_service_name: &str, answers: &[(&str, &str)]) -> String {
    let mut vars: BTreeMap<String, String> = BTreeMap::new();
    vars.insert("service_name".to_string(), docker_service_name.to_string());
    vars.insert("container_name".to_string(), docker_service_name.to_string());
    for (k, v) in answers {
        vars.insert((*k).to_string(), (*v).to_string());
    }
    expand_networks(&mut vars);
    substitute(template, &vars)
}

#[test]
fn rendu_caddy_identique_au_gabarit_nu() {
    let compose = render(
        CADDY_COMPOSE,
        "caddy-proxy",
        &[
            ("http_port", "80"),
            ("https_port", "443"),
            ("admin_port", "2019"),
            ("config_volume_path", "/home/docker/caddy"),
            ("admin_bind", "0.0.0.0:2019"),
            ("networks", "caddy-portainer,caddy-frontend"),
        ],
    );

    let expected = "services:
  caddy-proxy:
    image: caddy:2-alpine
    container_name: \"caddy-proxy\"
    restart: unless-stopped
    ports:
      - \"80:80\"     # HTTP
      - \"443:443\"   # HTTPS
      - \"2019:2019\"  # Admin
    networks:
      - caddy-portainer
      - caddy-frontend
    volumes:
      - \"/home/docker/caddy:/etc/caddy\"
    environment:
      - CADDY_ADMIN=0.0.0.0:2019

networks:
  caddy-portainer:
    external: true
  caddy-frontend:
    external: true
";

    assert_eq!(compose, expected);
}

#[test]
fn rendu_vaultwarden_container_name_ignore_toute_saisie_dediee() {
    // Vaultwarden déclare sa propre variable "container_name" dans template.yml (avec sa
    // propre description/exemple), mais generate-compose (nu comme ici) ne la lit jamais
    // en pratique : la valeur vient toujours de docker_service_name.
    let compose = render(
        VAULTWARDEN_COMPOSE,
        "vw-cocotte",
        &[
            ("data_volume_path", "./vw-data"),
            ("exposed_port", "80"),
            ("network", "caddy-network"),
            ("domain", "https://vault.cocotte.com"),
        ],
    );

    let expected = "services:
  vw-cocotte:
    image: vaultwarden/server:latest
    container_name: \"vw-cocotte\"
    restart: unless-stopped
    environment:
      DOMAIN: \"https://vault.cocotte.com\"
    volumes:
      - \"./vw-data:/data/\"
    expose:
      - \"80\"
    networks:
      - caddy-network

networks:
  caddy-network:
    external: true
";

    assert_eq!(compose, expected);
}
