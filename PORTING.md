# Plan de portage PurposeOps → Rust

Objectif : remplacer progressivement le module Nushell par un binaire Rust (`ppo-rs/`),
module par module, **sans big-bang** : les deux outils coexistent en lisant/écrivant les
mêmes YAML de `PurposeOps-config/`. Chaque étape est atomique = un commit, un critère
« fait quand » vérifiable.

**Répartition** : `[Toi]` = étape formatrice que tu codes (avec Claude en support),
`[Claude]` = mécanique ou risqué, implémenté par Claude et relu par toi.

## Vision long terme (cible)

À terme, `ppo` devient un outil de gestion de flotte Odoo/Docker complet. Le portage est
volontairement fait **avant** d'écrire les nouvelles features — pour ne pas les coder deux
fois, et parce que Rust est un meilleur terrain pour ce qui suit.

- **Parité** (Phases 1–6) : tout ce que le module nu fait aujourd'hui.
- **Bascule** (Phase 7) : on arrête d'utiliser le nu. La contrainte de coexistence
  (YAML en clair, sockets partagés) tombe — ce qui **débloque le chiffrement**.
- **Nouvelles capacités** (Phases 8–12), construites uniquement en Rust :
  1. Chiffrement des credentials au repos (clé hors repo).
  2. Provisioning end-to-end (template → compose → push → `up`).
  3. Vue de flotte / health multi-hôtes.
  4. Backups automatisés (planification + rotation).
  5. TUI (`ratatui`) en surcouche du **même** socle CLI.

Les Phases 8–12 sont **indépendantes entre elles** : leur ordre peut suivre ta motivation
du moment. Une seule dépendance dure : le chiffrement (8) doit venir **après** la bascule
(7), sinon il casse la lecture des mots de passe côté nu pendant la coexistence.

## Principes transverses

- **Nom du binaire : `ppor` pendant la migration** (le namespace `ppo <cmd>` est occupé
  par le module nu chargé dans le shell interactif). Renommage en `ppo` à la bascule finale.
- **Crates** : `clap` (derive) · `serde` + `serde_yaml_ng` · `inquire` (prompts + fuzzy
  select, remplace `input list` ET fzf) · `anyhow` (erreurs). Pas d'async : tout est
  sous-processus bloquants, `std::process::Command` suffit.
- **Ports SSH / DB restent des `String`** dans les structs (le YAML actuel contient
  `port: ''` pour localhost et `'5432'` quoté) — ne pas typer en `u16` avant la fin de la
  migration, sinon le round-trip casse le YAML pour le côté nu.
- **Tests** : unitaires pour ce qui est pur (quoting shell, round-trips serde, formatage du
  prompt). Tout ce qui touche un hôte distant se vérifie **en live** sur l'infra réelle
  (pas de suite de tests aujourd'hui — c'est déjà la règle du projet).
- **Test de coexistence systématique** : après toute étape qui écrit un YAML, vérifier que
  le côté nu relit correctement le fichier écrit par Rust (et inversement).

## Schéma des structs (relevé sur les YAML réels)

```rust
struct Host { name, hostname, user, port: String, identity_file: String, arch, docker_context }
// hosts.yaml = BTreeMap<String, Host> (clé = host_id: localhost, ngner, mcm, ca)

struct Customer { abbreviation: String, deployments: Vec<Deployment>, hosts: Vec<CustomerHost> }
struct CustomerHost { host_id: String, path_on_host: String }
// customers.yaml = BTreeMap<String, Customer>

struct Deployment {
    service_name: String,
    hosts: Vec<DeploymentHost>,        // { host_id, path_for_service, path_for_docker_compose }
    deployment_id: String,
    container_name: Option<String>,    // les 4 champs DB sont absents pour
    db_container_name: Option<String>, // les services sans base (Vaultwarden, Caddy)
    database_name: Option<String>,
    db_credentials: Option<DbCredentials>, // { host, port: String, user, password }
}

struct Context {
    host: BTreeMap<String, Host>,      // une seule clé = host_id courant
    prompt_show: bool,
    customer: BTreeMap<String, CustomerLite>, // vide {} si aucun ; CustomerLite = { abbreviation }
    deployment: Option<Deployment>,    // null, record complet, ou ANCIEN format string →
}                                      // enum untagged { Legacy(String), Full(Deployment) }

struct Service { template_dir_path, template_compose_path, variables: Vec<...> }
// services.yaml = BTreeMap<String, Service>
```

---

## Phase 0 — Bootstrap `[fait par Claude]`

- [x] **0.1** Toolchain déjà présente : cargo 1.93, rustc 1.93. (rust-analyzer à activer
      dans ton éditeur quand tu seras sur le laptop.)
- [x] **0.2** Crate `ppo-rs/` créé (`name = "ppor"`, édition 2024), déps ajoutées
      (`clap` derive · `serde` derive · `serde_yaml_ng` · `inquire` · `anyhow`),
      `ppo-rs/target/` ajouté au `.gitignore`. `cargo run` OK.
      **Reste à faire par toi** : `git add` + commit initial quand tu veux.

## Phase 1 — Binaire de prompt (gain immédiat) `[Toi → fait par Claude à ta demande]`

*Concepts (à relire dans le code, il est commenté pour ça) : structs, derive, `serde`,
`Result`/`Option`, pattern matching, `#[serde(untagged)]`.*

- [x] **1.1** Structs `Context`/`Host`/`Deployment`/`DbCredentials`/`DeploymentField`
      dans `src/config.rs`, `serde`, load/save. `deployment: null` (Option) et ancien
      format string (enum `#[serde(untagged)]`) gérés. Tests dans `src/config/tests.rs`.
      *Fait* : parse le YAML réel + null + legacy + round-trip qui préserve le mot de passe.
- [x] **1.2** Logique portée dans `src/prompt.rs` (`format_prompt` pur + `get_prompt_context`
      qui lit le disque). Tests des 7 cas dans `src/prompt/tests.rs`.
      *Fait* : `cargo test` = 11/11 vert ; sortie **identique** à `ppo p` (`🌐 vps-mcm - moi
      (odoo-perso)` vérifié en croisé nu↔rust).
- [x] **1.3** `clap` dans `src/main.rs` : `ppor prompt` + `ppor toggle-prompt` (alias `t`).
      *Fait* : toggle croisé vérifié (ppor écrit → nu relit, et inversement) ; le fichier
      écrit par Rust préserve tout (deployment + creds), même quoting `'2222'` que le nu.
- [x] **1.4** `~/.config/starship.toml` `[custom.ppo_context]` pointe désormais sur
      `ppo-rs/target/release/ppor prompt` (ancienne commande `nu -c` conservée en commentaire).
      *Fait* : ~1,2 ms/prompt contre ~58 ms avant (mesuré, ~48×).

## Phase 2 — Couche config + commandes de lecture/sélection `[Toi]`

*Concepts : modules Rust, ownership sur les maps, I/O fichiers, `inquire`.*

- [ ] **2.1** Module `config` : chemins (portés depuis `config/config.nu`, `~` résolu via
      `std::env::home_dir`/crate `home`) + parsing typé des 4 YAML. Bonus nouveau :
      `ppor check` qui parse tout et signale les incohérences (host_id inconnu, etc.).
      *Fait quand* : `ppor check` passe au vert sur la config réelle.
- [ ] **2.2** Commandes **lecture seule** : `hname`, `h`, `lsh`, `c`, `cname`, `lsc`,
      `lsd`, `pde`, `pdei`, `lss` (sous-commandes clap avec `visible_alias`).
      *Fait quand* : sorties équivalentes aux commandes nu correspondantes.
- [ ] **2.3** Commandes de **sélection** (écriture du contexte) : `sh` (set-host), `sc`
      (set-customer), `sd` (set-deployment) avec `inquire::Select` fuzzy. `sd` stocke le
      **record complet** du déploiement (schéma actuel post-migration).
      *Fait quand* : sélection croisée — `ppor sh` puis `ppo hname` côté nu, et
      inversement — cohérente dans les deux sens.

## Phase 3 — SSH ControlMaster `[Mixte : toi la structure, revue ensemble]`

*Concepts : `std::process::Command`, codes de retour, gestion de fichiers/sockets.*

Décision : **répliquer le schéma actuel** (sockets `controlmasters/user@hostname:port`,
`ssh -M -N -f` / `-O check` / `-O exit`) plutôt que le crate `openssh` — les sockets
restent ainsi **partagés avec le côté nu** pendant la coexistence. `openssh` reste une
option post-bascule.

- [ ] **3.1** `is_master_active`, `create_master_connection`, `run_with_master`
      (mêmes chemins de sockets que `ssh-manager.nu`). Les commandes distantes passent en
      arguments de `Command` — plus d'échappement `{{`/`}}` à la main.
      *Fait quand* : `ppor` exécute `uptime` sur un VPS **en réutilisant un socket créé
      par le côté nu**, et inversement.
- [ ] **3.2** `close`, `closeall`, `lsconn` (parsing des noms de sockets).
      *Fait quand* : parité avec les trois commandes nu.

## Phase 4 — Docker `[Toi]`

- [ ] **4.1** Équivalent de `run_docker_command` : dispatch localhost (`Command` direct)
      vs distant (chaîne construite avec un `shell_quote` **testé unitairement** —
      c'est LE test qui tue la classe de bugs historique du projet).
      *Fait quand* : `ppor dps` correct en local et sur un VPS ; `cargo test` couvre
      espaces, quotes simples, `$`, parenthèses.
- [ ] **4.2** `dstop`/`dstart`/`drestart` (fuzzy select du conteneur — parser
      `docker ps --format json` plutôt que le ssv), `dnls`, `dn extract`.
      *Fait quand* : cycle stop/start vérifié sur un conteneur non critique.

## Phase 5 — CRUD config `[Mixte]`

- [ ] **5.1** `[Toi]` `cc` (create_customer) : wizard `inquire`, préversion YAML,
      confirmation, insertion dans `customers.yaml` via le modèle typé complet
      (lecture → modification → réécriture du fichier entier, comme aujourd'hui).
      *Fait quand* : client créé par `ppor cc` visible dans `ppo lsc` côté nu.
- [ ] **5.2** `[Claude]` `ch`, `cs`, `dh`, `dc` — répétition mécanique du pattern 5.1.
      *Fait quand* : parité, testée sur des entrées jetables.
- [ ] **5.3** `[Claude]` `cdep` (create_deployment) : validation host existant + unicité
      **globale** du `deployment_id`, champs DB optionnels selon le type de service.
      *Fait quand* : refus d'un id dupliqué + création complète d'un déploiement scratch.

## Phase 6 — Backup / restore `[Claude, relu par toi]`

Le plus gros (455 l.) et le plus dangereux (`DROP DATABASE`). En dernier, une fois les
patterns rodés.

- [ ] **6.1** `backup run` : pg_dump via `docker exec -e PGPASSWORD`, tar du filestore,
      rapatriement. *Fait quand* : archive produite par `ppor` comparée octet à octet
      (contenu) à une archive produite par le nu sur le même déploiement.
- [ ] **6.2** `backup restore` : port de `do-generic-restore` (drop/create DB, docker cp
      sur conteneur arrêté, chown post-redémarrage, confirmation + `--force`).
      *Fait quand* : restauration réussie **sur un déploiement scratch d'abord**, puis
      validation en conditions réelles.
- [ ] **6.3** Fermeture de la parité : remplacer la palette `ppos` (fzf) par
      `ppor --help` + complétions shell générées (`clap_complete` sait produire du
      **nushell**). La lecture des services est déjà couverte (2.2) ; le rendu de
      templates part en Phase 9 (provisioning).
      *Fait quand* : `ppor` fait tout ce que le module nu faisait au quotidien ; plus
      aucune commande nu nécessaire.

## Phase 7 — Bascule

- [ ] **7.1** `cargo install --path ppo-rs`, renommer le binaire en `ppo`, retirer le
      `use ppo.nu` de `~/.config/nushell/config.nu`, définir les alias courts côté shell
      si souhaité (attention : `sh` en externe entre en collision avec le shell Bourne —
      soit garder ces alias confinés dans la config nu, soit renommer `sh` ; à trancher
      à ce moment-là).
- [ ] **7.2** Après quelques semaines sans retour arrière : archiver le code nu
      (tag git), mettre à jour CLAUDE.md. **À partir d'ici la coexistence est finie** :
      Rust est seul à lire/écrire les YAML → le chiffrement (Phase 8) devient possible.

---

# Nouvelles features (Phases 8–12) — indépendantes, ordre au choix

## Phase 8 — Chiffrement des credentials au repos `[Mixte]`

*Débloqué par la bascule (le nu ne lit plus les mots de passe en clair).* Approche :
chiffrer **uniquement les valeurs** `db_credentials.password` (et tout secret) dans les
YAML ; clé **hors repo**. Architecture clé : chiffrer/déchiffrer **à la frontière de la
couche config** (Phase 2) — backup et provisioning ne voient que du clair en mémoire et
ne touchent jamais à la crypto.

- [ ] **8.1** `[Toi]` Mécanisme de clé + crate `age` : clé dans `~/.config/ppo/key`
      (chmod 600) ou trousseau système. `encrypt_secret`/`decrypt_secret` testées
      unitairement.
      *Fait quand* : `cargo test` prouve chiffrer→déchiffrer = identité.
- [ ] **8.2** `[Claude]` `ppor secrets encrypt` : lit les YAML, chiffre les champs
      sensibles (préfixe marqueur `enc:` pour distinguer clair/chiffré), réécrit. Le
      loader déchiffre à la volée, le writer rechiffre.
      *Fait quand* : plus aucun mot de passe en clair dans les YAML, tout marche encore,
      et retirer la clé fait tout échouer (preuve que ça protège réellement).

## Phase 9 — Provisioning end-to-end `[Mixte]`

Déployer un nouveau service en une commande. Réutilise SSH (3), Docker (4), CRUD (5).

- [ ] **9.1** `[Toi]` Port du `templater` : rendu d'un template (`templates/<Service>/`)
      avec substitution de variables → `compose.yaml` local.
      *Fait quand* : compose rendu identique à celui du templater nu (Vaultwarden).
- [ ] **9.2** `[Claude]` `ppor provision` : rendu → création du dossier distant (SSH) →
      push du compose → `docker compose up -d` → enregistrement du déploiement dans
      `customers.yaml` (CRUD 5.3). Confirmation avant `up`.
      *Fait quand* : un service scratch déployé de bout en bout sur un VPS de test.

## Phase 10 — Vue de flotte / health `[Toi]`

Nouvelle capacité : l'état de tous les hôtes d'un coup.

- [ ] **10.1** `ppor fleet status` : pour chaque hôte de `hosts.yaml`, **en parallèle**
      (`std::thread` — pas besoin d'async), conteneurs up/down + uptime + disque, en
      tableau. Hôtes injoignables signalés, pas bloquants.
      *Fait quand* : tableau lisible de toute la flotte.
- [ ] **10.2** (optionnel) `ppor fleet logs` / `fleet exec` : tail de logs et commande
      ad hoc ciblée par client/déploiement.
      *Fait quand* : logs d'un déploiement rapatriés sans session SSH manuelle.

## Phase 11 — Backups automatisés `[Claude, relu par toi]`

Réutilise le backup (6) + secrets (8).

- [ ] **11.1** Rotation/rétention : nommage horodaté, purge au-delà de N backups.
      *Fait quand* : N backups gardés, plus vieux purgés automatiquement.
- [ ] **11.2** Planification : `ppor backup all` (tous les déploiements DB) appelé par un
      timer système (cron/systemd), + rapport succès/échec.
      *Fait quand* : un job nocturne backup toute la flotte et notifie en cas d'échec.

## Phase 12 — TUI `[Toi, gros morceau]`

Surcouche `ratatui` sur le socle CLI (le CLI reste la référence scriptable).

- [ ] **12.1** `ppor tui` : navigation hôte → client → déploiement, sélection visuelle
      qui écrit le contexte.
      *Fait quand* : changer de contexte sans taper de commande.
- [ ] **12.2** Actions depuis la TUI : status flotte, start/stop conteneur, lancer un
      backup, avec confirmations.
      *Fait quand* : les opérations courantes se font sans quitter la TUI.
