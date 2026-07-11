# Plan de portage PurposeOps → Rust

Objectif : remplacer progressivement le module Nushell par un binaire Rust (`ppo-rs/`),
module par module, **sans big-bang** : les deux outils coexistent en lisant/écrivant les
mêmes YAML de `PurposeOps-config/`. Chaque étape est atomique = un commit, un critère
« fait quand » vérifiable.

**Répartition** : à partir de la Phase 5, toutes les étapes sont `[Claude]` — implémentées
par Claude et relues par toi (les tags `[Toi]`/`[Mixte]` restants dans les phases 1–4
ci-dessous sont conservés tels quels, c'est l'historique réel de ce qui a été codé).

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

## Phase 2 — Couche config + commandes de lecture/sélection `[Toi → fait par Claude à ta demande]`

**Fait** : 16 tests verts (parsing, check vert+rouge, prompt) ; toutes les commandes de
lecture vérifiées contre le nu ; sélection (`sh`/`sc`/`sd`) croisée nu↔rust cohérente et
état restauré. `ppor check` (nouveau) vert sur la config réelle.


*Concepts : modules Rust, ownership sur les maps, I/O fichiers, `inquire`, `Result`.*

Structure cible : `config.rs` grossit (modèles + loaders), un module par domaine miroir du
nu (`host.rs`, `customer.rs`, `deployment.rs`, `service.rs`), `check.rs`, `ui.rs` (le
sélecteur fuzzy, équivalent de `config-helper.nu` `select_item`).

- [x] **2.1** Étendre `config.rs` : chemins `hosts`/`customers`/`services`, modèles
      manquants (`Customer` = abbreviation + `deployments: Vec<Deployment>` + `hosts:
      Vec<CustomerHost>`, `CustomerHost`, `Service`), loaders `load_hosts`/`load_customers`/
      `load_services`. Tests de parsing des 3 fichiers réels dans `config/tests.rs`.
      *Fait quand* : `cargo test` parse hosts/customers/services réels (dont `deployments: []`
      de Multibikes et les champs DB optionnels).
- [x] **2.2** `ppor check` (`check.rs`, capacité nouvelle) : chaque `host_id` de
      customers/context existe dans hosts ; `deployment_id` uniques globalement ; le
      contexte pointe vers un host/customer valides. Rapport lisible, exit ≠ 0 si incohérent.
      *Fait quand* : vert sur la config réelle ; rouge si on casse un host_id à la main.
- [x] **2.3** `ui.rs` : `select(prompt, items) -> Option<String>` via `inquire::Select`
      (filtre fuzzy intégré) — remplace `select_item` **et** le fallback fzf.
      *Fait quand* : un `ppor` de test affiche un menu fuzzy et renvoie le choix.
- [x] **2.4** Lecture — **hosts** (`host.rs`) : `get_current_host` → `h`/`hname`, `lsh`
      (host, name, type local/remote, courant). `visible_alias` clap.
      *Fait quand* : `h`/`hname`/`lsh` donnent les mêmes infos que côté nu.
- [x] **2.5** Lecture — **customers** (`customer.rs`) : `c`/`cname`, `lsc`.
      *Fait quand* : parité d'info avec `c`/`lsc` nu.
- [x] **2.6** Lecture — **deployments** (`deployment.rs`) : `pde` (id courant), `pdei`
      (record ; erreur explicite si absent ou ancien format string), `lsd` (déploiements
      du client courant).
      *Fait quand* : parité, et `pdei` refuse proprement un contexte sans déploiement.
- [x] **2.7** Lecture — **services** (`service.rs`) : `lss` (noms des services).
      *Fait quand* : même liste que `lss` nu.
- [x] **2.8** Sélection — `sh` (set-host) : arg direct **ou** menu fuzzy ; écrit
      `context.host = {host_id: <record de hosts.yaml>}`.
      *Fait quand* : `ppor sh <id>` puis `ppo hname`/`p` côté nu voient le nouvel hôte.
- [x] **2.9** Sélection — `sc` (set-customer) : écrit `context.customer = {name:
      <customer moins deployments/hosts>}`. Vérif cohérence hôte↔client ; si l'hôte courant
      n'est pas un hôte du client, proposer (`inquire::Confirm`) de basculer l'hôte aussi.
      *Fait quand* : `ppor sc <name>` cohérent, et le prompt de bascule d'hôte fonctionne.
- [x] **2.10** Sélection — `sd` (set-deployment) : exige un client sélectionné ; écrit le
      **record complet** du déploiement ; même logique de cohérence d'hôte que `sc`.
      *Fait quand* : sélection croisée nu↔rust cohérente ; `ppo pdei` relit le record écrit.

## Phase 3 — SSH ControlMaster `[Mixte → fait par Claude à ta demande]`

*Concepts : `std::process::Command`, codes de retour, gestion de fichiers/sockets.*

Décision : **répliquer le schéma actuel** (sockets `controlmasters/user@hostname:port`,
`ssh -M -N -f` / `-O check` / `-O exit`) plutôt que le crate `openssh` — les sockets
restent ainsi **partagés avec le côté nu** pendant la coexistence. `openssh` reste une
option post-bascule.

Note : `is_master_active`/`create_master_connection`/`run_with_master` sont des fonctions
internes côté nu aussi (aucune commande `ppo` publique ne les appelle directement — seuls
`docker/core.nu` et `backup.nu` le font, Phases 4/6). Donc pas de nouvelle commande CLI
ici ; leur preuve de fonctionnement passe par un test d'intégration `#[ignore]` (réseau
réel), lancé manuellement.

- [x] **3.1** `src/ssh.rs`. Les commandes distantes passent en un seul argument à `ssh`
      (le shell distant les reçoit intactes) — plus d'échappement manuel de quoting côté
      Rust, seul l'échappement `{{`/`}}` du nu est reproduit à l'identique.
      **Fait, vérifié en live dans les deux sens** : (a) le nu crée le master vers mcm,
      `cargo test -- --ignored live_run_with_master_uptime_on_mcm` l'a réutilisé
      (exécution en 0.07 s, socket inchangé) ; (b) après `closeall`, le test Rust a recréé
      le master, et un `run_with_master` lancé depuis le nu l'a réutilisé (socket
      identique avant/après, aucune recréation).
- [x] **3.2** `close`/`closeall`/`lsconn` (clap, alias identiques au nu), parsing des noms
      de sockets `user@hostname:port` testé unitairement (cas valide + invalide).
      *Fait* : `ppor closeall` a fermé la connexion réelle, `ppor lsconn` confirme vide,
      `ppor close` testé sur la connexion courante (mcm).
      *Fait quand* : parité avec les trois commandes nu.

## Phase 4 — Docker `[Toi]` — ✅ fait

**Fait** : les bras `match` manquants dans `fn main()` ont été ajoutés (`docker::cmd_start()?`,
`cmd_stop`, `cmd_restart`, `cmd_dn_extract`, `cmd_dps`, `cmd_dnls`), `cargo build` compile
proprement. Alias `dn extract` tranché : reste `dnextract` (un mot), cohérent avec les
autres alias clap (`dstart`/`dstop`/`dps`/`dnls`) plutôt qu'une sous-commande imbriquée
pour un seul cas isolé.

`cargo test` : 28/28 verts (dont tous les `docker::tests::*`, logique pure de
`shell_quote`). Vérifications live :
- `ppor dps` sur mcm (distant, via ControlMaster) : 42 conteneurs, **identiques en nom/
  image/statut** à `nu -c 'use ppo.nu; ppo dps'` sur le même hôte.
- `ppor dps <filtre>` et `--ports`, `ppor dnls` : corrects, `dnls` identique côté nu
  (31 réseaux, même ordre).
- Dispatch localhost vérifié (`ppor sh localhost` puis `ppor dps`) en plus du dispatch
  distant.
- Cycle complet `dstop`/`dstart` sur `odoo-demo2` (conteneur de test localhost, non
  critique) : piloté via `pexpect` (le menu fuzzy `inquire` a besoin d'un vrai pty,
  pas testable par simple pipe de stdin) — arrêt confirmé par disparition de `dps`,
  puis redémarrage confirmé par réapparition (`Up 10 seconds`).
- Contexte restauré sur `mcm` (hôte sélectionné avant l'intervention) à la fin.

- [x] **4.1** Équivalent de `run_docker_command` : dispatch localhost (`Command` direct)
      vs distant (chaîne construite avec un `shell_quote` **testé unitairement** —
      c'est LE test qui tue la classe de bugs historique du projet).
      *Fait* : `ppor dps` correct en local et sur un VPS ; `cargo test` couvre
      espaces, quotes simples, `$`, parenthèses.
- [x] **4.2** `dstop`/`dstart`/`drestart` (fuzzy select du conteneur — parser
      `docker ps --format json` plutôt que le ssv), `dnls`, `dn extract`.
      *Fait* : cycle stop/start vérifié en live sur un conteneur non critique.

## Phase 5 — CRUD config `[Claude]` — ✅ fait

**Fait** : `save_yaml_map`/`delete_from_map` génériques ajoutés à `config.rs` (port de
`config-helper.nu`'s `delete`), `ui::text` ajouté pour les prompts libres. `cc`/`ch`/`cs`
+ `dc`/`dh`/`ds` (host.rs/customer.rs/service.rs) et `cdep` (deployment.rs) implémentés
et câblés dans `main.rs`. Écart volontaire vs le nu : `insert` sur une clé déjà
existante est refusé explicitement (`❌ ... already exists`) plutôt que de planter comme
le ferait `record | insert` côté nu — un garde-fou anti-écrasement accidentel, pas un
changement de comportement fonctionnel.

Vérifié en live (via `pexpect`, `inquire` a besoin d'un vrai pty) sur des entrées
jetables, avec relecture croisée côté nu à chaque étape, puis restauration exacte des
YAML (`customers.yaml`/`hosts.yaml`/`services.yaml` octet-identiques à l'état de départ,
`context.yaml` restauré via `sc`/`sd`) :
- `ch` + `dh` : hôte `scratchtest` créé (port `'2299'` bien quoté), vu par `ppo lsh`
  côté nu, puis supprimé.
- `cc` + `dc` : client `ScratchCustomer` créé, vu par `ppo lsc` côté nu, puis supprimé
  (avec ses déploiements).
- `cs` + `ds` : service `ScratchService` créé, vu par `ppo lss` côté nu, puis supprimé.
- `cdep` : déploiement sans DB (`scratchdep-01`) et avec DB (`scratchdep-db-01`, tous les
  champs `container_name`/`db_container_name`/`database_name`/`db_credentials` avec
  `port: '5432'` bien quoté) créés pour `ScratchCustomer`, relus via `ppo pdei` côté nu ;
  rejet confirmé d'un `deployment_id` dupliqué (`scratchdep-01` réutilisé → erreur, pas
  de doublon écrit).
- `ppor check` reste vert (5 clients, 5 déploiements) tout du long.

- [x] **5.1** `[Claude]` `cc` (create_customer) : wizard `inquire`, préversion YAML,
      confirmation, insertion dans `customers.yaml` via le modèle typé complet
      (lecture → modification → réécriture du fichier entier, comme aujourd'hui).
      *Fait quand* : client créé par `ppor cc` visible dans `ppo lsc` côté nu.
- [x] **5.2** `[Claude]` `ch`, `cs`, `dh`, `dc` — répétition mécanique du pattern 5.1
      (`ds` ajouté aussi pour la parité complète avec les alias `mod.nu`).
      *Fait quand* : parité, testée sur des entrées jetables.
- [x] **5.3** `[Claude]` `cdep` (create_deployment) : validation host existant + unicité
      **globale** du `deployment_id`, champs DB optionnels selon le type de service.
      *Fait quand* : refus d'un id dupliqué + création complète d'un déploiement scratch.

## Phase 6 — Backup / restore `[Claude, relu par toi]` — ✅ fait

Le plus gros (455 l.) et le plus dangereux (`DROP DATABASE`). En dernier, une fois les
patterns rodés.

- [x] **6.1** `backup run` : pg_dump via `docker exec -e PGPASSWORD`, tar du filestore,
      rapatriement. *Fait quand* : archive produite par `ppor` comparée octet à octet
      (contenu) à une archive produite par le nu sur le même déploiement.
      **Fait** : `src/backup.rs`, port de `do-generic-backup` (steps identiques : mkdir
      distant, `pg_dump` dans le conteneur DB, va-et-vient du `.sql` DB→hôte→APP,
      détection + tar du filestore (ou archive vide s'il est absent), archive globale,
      `docker cp` final vers l'hôte, nettoyage `-u root`), avec nettoyage best-effort en
      cas d'erreur à n'importe quelle étape (équivalent du `try`/`catch` nu). 3 écarts
      volontaires **sans changement de comportement**, documentés en tête de fichier :
      `--service` et `--silent` (jamais lus dans le corps nu, paramètres morts) et
      `--dbHost` côté `do-generic-backup` (jamais utilisé — `pg_dump` force `-h
      localhost`, cohérent puisqu'il tourne *dans* le conteneur DB) ne sont pas repris ;
      le bloc `🔍 DEBUG VARIABLES` de `backup run` (dump de debug laissé en place côté
      nu) n'est pas porté non plus.

      Vérifié en live sur le déploiement réel `Mael`/`odoo-perso` (hôte `mcm`, non
      destructif — dump + copies uniquement) : `ppor backup run` puis `ppo backup run`
      côté nu, à ~10s d'intervalle, comparaison des deux archives rapatriées sur le
      laptop — filestore `_fs.tar.gz` **identique octet à octet** (même MD5,
      2 823 838 octets), dump `.sql` identique sur les 106 981 lignes sauf les tokens
      aléatoires `\restrict`/`\unrestrict` que `pg_dump` régénère à chaque invocation
      (attendu, ça diffère aussi entre deux backups nu consécutifs). `cargo test` :
      4 nouveaux tests purs (`resolve_remote_path`, `check_step` succès/échec via
      `ExitStatusExt::from_raw`), 37/38 verts (1 `#[ignore]` réseau).
- [x] **6.2** `backup restore` : port de `do-generic-restore` (drop/create DB, docker cp
      sur conteneur arrêté, chown post-redémarrage, confirmation + `--force`).
      *Fait quand* : restauration réussie **sur un déploiement scratch d'abord**, puis
      validation en conditions réelles.
      **Fait** : `cmd_backup_restore` + `do_generic_restore`/`run_restore_steps` dans
      `src/backup.rs`, mêmes étapes que le nu (stop app → extraction de l'archive → DROP
      + CREATE DATABASE → restauration du dump SQL → restauration filestore via `docker
      cp` pendant que l'app est arrêtée (`exec` indisponible sur conteneur stoppé) →
      restart → `chown -R odoo:odoo` → nettoyage), avec le même best-effort de secours
      (redémarrage app + `rm -rf` du work_dir) sur erreur à n'importe quelle étape. La
      résolution croisée du chemin de backup (`~/backups/<abbrev>/<host_id>/` du client
      courant, ou chemin absolu si le backup vient d'un autre client/déploiement) et
      `list_remote_backups` (sélection fuzzy si aucun fichier passé en argument) sont
      aussi portés.

      **Scratch d'abord** (déploiement jetable `demo-odoo-restore-test`, créé via `cc`/
      `cdep` du Phase 5, ciblant les conteneurs locaux `odoo-demo`/`odoo-demo-db`) :
      confirmation testée en live (refus `n` → annulation propre, aucune commande
      destructive lancée), puis restauration **croisée** — la même archive
      `manual_catteaumael_*.tar.gz` du déploiement réel `Mael`/`odoo-perso` (hôte `mcm`)
      restaurée sur la base locale `demo`, avec `--force`. Vérifié après coup : la base
      cible contient bien les 366 tables et les données de production (`res_company` =
      "CATTEAU MAEL THIBAULT ERWAN"), filestore restauré (211 fichiers, `chown`é
      `odoo:odoo`), conteneur applicatif redémarré et sain (logs sans erreur, HTTP up).

      **Bug réel trouvé et corrigé en cours de route** (pas un bug de `ppor` — le nu
      original aurait buté sur exactement la même chose) : ce `docker-compose.yml` local
      force `user: "101:101"` pour le conteneur `odoo-demo`, alors que l'utilisateur
      `odoo` de l'image a l'UID **100** (GID 101) — donc le `chown -R odoo:odoo` de
      `do-generic-restore`/`run_restore_steps` (identique au nu) remet bien les fichiers
      à `100:101`, mais le process qui tourne réellement (UID 101, imposé par le
      `user:` du compose) n'a alors que lecture/exécution de groupe (`755`) sur ces
      répertoires — pas d'écriture. Odoo plantait donc en 500 en essayant de générer de
      nouveaux bundles d'assets JS dans le filestore restauré, ce qui bloquait le
      JavaScript qui démasque le formulaire de login (`d-none` jamais retiré → page de
      login visuellement vide). Corrigé manuellement pour ce test (`chown -R 101:101`
      + purge des `ir_attachment` de bundles cassés) — **spécifique à ce conteneur de
      démo local**, pas quelque chose à corriger dans `do_generic_restore` : les
      déploiements réels (mcm, ngner) n'ont pas ce `user:` explicite dans leur compose,
      donc le process tourne bien en tant que `odoo` (UID 100) et `chown -R odoo:odoo`
      cible le bon UID chez eux.

      `cargo build --release`, `cargo clippy`, `cargo test` restent verts (37/38, 1
      `#[ignore]` réseau) après l'ajout.
- [x] **6.3** Fermeture de la parité : remplacer la palette `ppos` (fzf) par
      `ppor --help` + complétions shell générées (`clap_complete` sait produire du
      **nushell**). La lecture des services est déjà couverte (2.2) ; le rendu de
      templates part en Phase 9 (provisioning).
      *Fait quand* : `ppor` fait tout ce que le module nu faisait au quotidien ; plus
      aucune commande nu nécessaire.

      **Fait** : `ppor --help` liste déjà chaque commande avec sa description **et** son
      alias court (généré par `clap` depuis les définitions réelles — ne peut pas dériver
      du code, contrairement à la liste `$commands` codée en dur dans `ppos`). Ajout de
      `ppor completions <shell>` (`clap_complete` + `clap_complete_nushell`) qui imprime
      sur stdout un script à sourcer pour bash/zsh/fish/elvish/powershell/nushell.
      Vérifié : `ppor completions nushell` chargé avec succès dans un `nu` réel (`use ... *`
      sans erreur) ; les 5 autres shells génèrent aussi sans erreur.

      **Limite connue, pas corrigée** : `clap_complete_nushell` 4.6.0 ne génère de
      complétion que pour les noms canoniques des sous-commandes (`set-host`), pas pour
      leurs `visible_alias` (`sh`) — vérifié dans les sources du crate, ce n'est pas un
      oubli de configuration. bash/zsh/fish les incluent (vérifié : `ppor,sh)` présent
      dans la sortie bash). Écrire un générateur nushell maison pour combler ce trou
      serait disproportionné pour ce que ça apporte ; `ppor --help` reste la référence
      pour découvrir les alias courts.

## Phase 7 — Bascule

- [x] **7.1** `cargo install --path ppo-rs`, renommer le binaire en `ppo`, retirer le
      `use ppo.nu` de `~/.config/nushell/config.nu`, définir les alias courts côté shell
      si souhaité (attention : `sh` en externe entre en collision avec le shell Bourne —
      soit garder ces alias confinés dans la config nu, soit renommer `sh` ; à trancher
      à ce moment-là).

      **Fait** : package Cargo renommé `ppor` → `ppo` (`Cargo.toml`, `#[command(name = ...)]`
      dans `main.rs`, + les quelques messages d'erreur/doc-comments qui citaient
      littéralement `ppor <cmd>` dans `check.rs`/`deployment.rs`/`main.rs` — pas touché
      aux mentions historiques de `ppor` dans les logs `Fait` des phases précédentes de
      ce fichier, qui décrivent fidèlement ce qui a été exécuté à l'époque).
      `cargo install --path .` → `~/.cargo/bin/ppo` (déjà sur le `PATH`).

      Décision sur la collision `sh`/Bourne shell : **pas d'alias top-level côté shell du
      tout**. Le nu module namespaçait déjà tout sous `ppo <cmd>` (`use ppo.nu` sans `*`),
      donc `ppo sh`/`ppo sc`/etc. existaient déjà uniquement comme *sous-commandes* de
      `ppo`, jamais comme commandes `sh`/`sc` nues dans le PATH ou le scope nu — même
      chose côté Rust via les `visible_alias` clap (`ppo sh` invoque `set-host`). Aucune
      collision possible avec `/bin/sh`, aucun renommage nécessaire.

      `~/.config/nushell/config.nu` : suppression de `use ~/dev/nu-modules/PurposeOps/ppo.nu`
      et `export alias "ppos" = ppo ppos` ; ajout d'une ligne miroir du pattern Starship
      déjà en place (`starship init nu | save -f (vendor/autoload/starship.nu)`) :
      `ppo completions nushell | save -f (vendor/autoload/ppo-completions.nu)`,
      régénérée à chaque lancement de shell donc jamais périmée.
      `~/.config/starship.toml` : `[custom.ppo_context]` pointe maintenant sur
      `/home/ngner/.cargo/bin/ppo prompt` (au lieu du chemin `target/release/ppor`).

      Vérifié : `nu -c '<script>'` ne charge PAS `config.nu` par défaut dans cette
      version de nushell (0.106.0) — contrairement à l'hypothèse initiale, confirmé en
      ajoutant un marqueur en tête de `config.nu` et en observant qu'il ne s'affichait
      pas sous `-c` seul. La vraie validation s'est donc faite avec une **session
      interactive réelle** (`pexpect` spawnant `nu` sans `-c`) : après suppression du
      fichier de complétions, une session interactive fraîche a bien régénéré
      `ppo-completions.nu` **et** `starship.nu` au même timestamp (donc `config.nu`
      s'exécute en entier sans erreur fatale) ; `nu-check` confirme un nu valide
      (79 `export extern`, zéro référence résiduelle à `ppor`). Prompt Starship
      vérifié rendu correctement via le nouveau binaire installé. Config PurposeOps
      (`hosts.yaml`/`customers.yaml`/`context.yaml`) non touchée par cette phase.
- [ ] **7.2** Après quelques semaines sans retour arrière : archiver le code nu
      (tag git), mettre à jour CLAUDE.md. **À partir d'ici la coexistence est finie** :
      Rust est seul à lire/écrire les YAML → le chiffrement (Phase 8) devient possible.

---

# Nouvelles features (Phases 8–12) — indépendantes, ordre au choix

## Phase 8 — Chiffrement des credentials au repos `[Claude]`

*Débloqué par la bascule (le nu ne lit plus les mots de passe en clair).*

**Décision de design (portabilité)** : `identity_file` est un chemin — copier
`PurposeOps-config/` sur une autre machine casse le SSH dès que la clé n'existe pas au
même endroit localement. Plutôt qu'un simple chiffrement de `db_credentials.password`,
le schéma de clés est étendu pour rendre la config **auto-portable** :

- **Une clé `age` par client** (`~/.config/ppo/keys/<client>.txt`, générée à la demande),
  pas une seule clé globale. Ça limite la casse si une clé fuit ou est partagée (ex :
  déléguer un client à un prestataire) — mais ça ne protège pas contre une machine
  compromise (toutes les clés vivraient dans le même dossier avec les mêmes
  permissions) ; ça reste un chiffrement **au repos**, pas un coffre-fort applicatif.
- `db_credentials.password` d'un déploiement → chiffré **uniquement** à la clé de son
  client (portée la plus étroite possible).
- Nouveau champ `Host.identity_key` (clé SSH privée chiffrée, en plus de `identity_file`
  gardé comme repli) → chiffré à **l'union** des clés de tous les clients ayant un
  déploiement sur cet hôte, car `hosts.yaml` n'est **pas** un mapping 1:1 client↔hôte
  (`ngner` héberge à la fois Cocotte et Sylvie ; `mcm` héberge à la fois Mael et
  Multibikes). `age` gère nativement le chiffrement multi-destinataires — un seul
  ciphertext, plusieurs clés peuvent le déchiffrer.
- Déchiffrement : essayer chaque identité locale présente dans `~/.config/ppo/keys/`
  jusqu'à ce qu'une fonctionne ; pas besoin d'indiquer quelle clé a chiffré quoi.
- Architecture : chiffrer/déchiffrer **à la frontière de la couche config** (Phase 2) —
  backup et provisioning ne voient que du clair en mémoire, jamais la crypto.
- Pas de révocation automatique à la suppression d'un client/déploiement (ré-chiffrer
  pour retirer un destinataire n'efface pas le clair qu'il a déjà pu voir) — juste une
  recomputation de l'ensemble de destinataires **à la création** d'un lien
  client↔hôte (`cdep`). Rotation manuelle si un jour nécessaire, hors scope pour l'instant.

- [x] **8.1** `[Claude]` Mécanisme de clé + crate `age` : génération/chargement d'identités
      par client (`~/.config/ppo/keys/<client>.txt`, chmod 600), `encrypt_secret`
      (multi-destinataires) / `decrypt_secret` (essaie chaque identité locale) avec
      marqueur `enc:` pour distinguer clair/chiffré. Testées unitairement.
      *Fait quand* : `cargo test` prouve chiffrer→déchiffrer = identité pour un seul
      destinataire **et** pour plusieurs (n'importe laquelle des identités déchiffre).

      **Fait** : `src/secrets.rs`. API `age` "full" (`Encryptor::with_recipients` +
      `Decryptor::decrypt`, pas le raccourci `age::encrypt`/`decrypt` qui n'accepte qu'un
      seul destinataire/identité) — utilisée uniformément même pour le cas à un seul
      destinataire, pour garder `encrypt_secret`/`decrypt_secret` génériques sur le nombre
      de destinataires. Ciphertext encodé en base64 (pas le format "armor" `age`, pensé
      pour des fichiers `.age` autonomes, pas pour être imbriqué dans une valeur YAML).
      7 tests unitaires purs : round-trip à un et plusieurs destinataires (chaque identité
      déchiffre indépendamment), une identité non-destinataire échoue, aucune identité
      locale donne une erreur explicite plutôt qu'un échec silencieux, valeur non
      préfixée `enc:` rejetée, chiffrement sans destinataire refusé. `load_*`/`save_*`
      (I/O disque vers `~/.config/ppo/keys/`) pas testés unitairement — même convention
      que `config.rs` (I/O fichiers vérifié en live, pas en test, dans ce projet) ; câblés
      et vérifiés en live en 8.2/8.3/8.4. `cargo test` : 44/45 verts (1 `#[ignore]` réseau).
- [x] **8.2** `[Claude]` Intégration couche config : `Host.identity_key: Option<String>`
      (repli sur `identity_file` si absent), `DbCredentials.password` déchiffré à la
      volée au chargement / rechiffré à l'écriture. Un champ chiffré dont aucune identité
      locale ne peut être déchiffrée ne doit pas casser les commandes qui ne l'utilisent
      pas (`lsh`/`lsc`/`check`...), seulement échouer proprement quand il est réellement
      utilisé.
      *Fait quand* : round-trip préserve les champs chiffrés ; un déploiement dont la
      clé SSH/DB est illisible localement échoue clairement à l'usage, pas au chargement.

      **Fait** : `Host.identity_key` ajouté (`config.rs`, `skip_serializing_if` comme les
      autres champs optionnels). `secrets::reveal(&str) -> Result<String>` ajouté : passe
      une valeur non chiffrée telle quelle (tolère le clair pas encore migré, 8.4), sinon
      déchiffre avec les identités locales — c'est cet accesseur, appelé au point d'usage
      réel, qui porte le "échoue seulement à l'usage", pas `load_hosts`/`load_customers`.
      `cdep` chiffre désormais le mot de passe DB immédiatement à la clé du client
      (généré si absent) avant de l'écrire dans `customers.yaml` ; `backup run`/`backup
      restore` appellent `secrets::reveal` sur `db_credentials.password` avant de
      construire la commande `pg_dump`/`psql`. Le champ `identity_key` lui-même n'est pas
      encore consommé par `ssh.rs` — ça arrive en 8.3, en même temps que le mécanisme qui
      le peuple pour la première fois (pas de sens à câbler la consommation d'un champ
      toujours vide).

      Vérifié en live sur un déploiement scratch (`EncTestCustomer` → conteneurs locaux
      `odoo-demo`/`odoo-demo-db`, nettoyé après coup) : `cdep` a bien produit un
      `db_credentials.password` préfixé `enc:` dans l'aperçu YAML **et** dans
      `customers.yaml` ; clé générée à `~/.config/ppo/keys/EncTestCustomer.txt` en
      `0600` ; `ppo backup run` a réussi de bout en bout (déchiffrement → `pg_dump`
      authentifié avec succès) ; `ppo pdei`/`ppo lsc` continuent de fonctionner sans
      toucher au déchiffrement (`pdei` affiche le blob chiffré tel quel). Déplacer le
      fichier de clé fait échouer `backup run` avec une erreur claire (`aucune clé locale
      ne peut déchiffrer cette valeur`) sans toucher `pdei`/`lsc` ; le remettre en place
      restaure le fonctionnement normal. 2 tests unitaires ajoutés pour le round-trip du
      champ `identity_key` (`config/tests.rs`) ; `cargo test` : 46/47 verts.
- [ ] **8.3** `[Claude]` Calcul des destinataires : `cdep` (re)chiffre `identity_key` de
      l'hôte visé pour inclure la clé publique du client, en plus des clés déjà
      présentes ; génère la clé du client si elle n'existe pas encore.
      *Fait quand* : deux clients partageant un hôte déchiffrent chacun sa clé SSH avec
      leur propre fichier de clé, indépendamment l'un de l'autre.
- [ ] **8.4** `[Claude]` `ppo secrets encrypt` : migration en place de la config réelle
      (génère les clés clients manquantes, chiffre `identity_file`→`identity_key` et
      `db_credentials.password`, réécrit les YAML).
      *Fait quand* : plus aucun secret en clair dans `PurposeOps-config/` ; retirer le
      fichier de clé d'un client fait échouer exactement (et seulement) ce qui dépend de
      lui (preuve que le cloisonnement par client fonctionne réellement).

## Phase 9 — Provisioning end-to-end `[Claude]`

Déployer un nouveau service en une commande. Réutilise SSH (3), Docker (4), CRUD (5).

- [ ] **9.1** `[Claude]` Port du `templater` : rendu d'un template (`templates/<Service>/`)
      avec substitution de variables → `compose.yaml` local.
      *Fait quand* : compose rendu identique à celui du templater nu (Vaultwarden).
- [ ] **9.2** `[Claude]` `ppor provision` : rendu → création du dossier distant (SSH) →
      push du compose → `docker compose up -d` → enregistrement du déploiement dans
      `customers.yaml` (CRUD 5.3). Confirmation avant `up`.
      *Fait quand* : un service scratch déployé de bout en bout sur un VPS de test.

## Phase 10 — Vue de flotte / health `[Claude]`

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

## Phase 12 — TUI `[Claude, gros morceau]`

Surcouche `ratatui` sur le socle CLI (le CLI reste la référence scriptable).

- [ ] **12.1** `ppor tui` : navigation hôte → client → déploiement, sélection visuelle
      qui écrit le contexte.
      *Fait quand* : changer de contexte sans taper de commande.
- [ ] **12.2** Actions depuis la TUI : status flotte, start/stop conteneur, lancer un
      backup, avec confirmations.
      *Fait quand* : les opérations courantes se font sans quitter la TUI.
