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
- [x] **5.4** `[Claude]` `ddep` (delete_deployment) — **capacité nouvelle, ajoutée après
      coup** : ni le nu ni `ppo` avant ce commit n'avaient de suppression de déploiement
      (`deployment-manager/mod.nu` n'exposait jamais cet alias). Sélection fuzzy ou id
      direct dans les déploiements du client courant, aperçu YAML, confirmation. Alias
      `ddep` et pas `dd`, même logique que `cdep`/`cd` : éviter de masquer l'utilitaire
      shell `dd` (bien plus dangereux à collisionner que `cd`). Si le déploiement
      supprimé est celui actuellement sélectionné dans `context.yaml` (record complet,
      pas juste l'id), le contexte est désélectionné pour ne pas laisser de référence
      pendante que `pdei`/`backup` liraient encore.
      *Fait quand* : suppression testée en live (confirmation refusée → rien ne change ;
      id direct et sélection fuzzy tous deux fonctionnels ; supprimer le déploiement
      courant vide bien `context.yaml.deployment`, vérifié par `pdei` qui se remet à
      erreurer proprement).

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
- [x] **8.3** `[Claude]` Calcul des destinataires : `cdep` (re)chiffre `identity_key` de
      l'hôte visé pour inclure la clé publique du client, en plus des clés déjà
      présentes ; génère la clé du client si elle n'existe pas encore.
      *Fait quand* : deux clients partageant un hôte déchiffrent chacun sa clé SSH avec
      leur propre fichier de clé, indépendamment l'un de l'autre.

      **Fait** : `ensure_host_key_encrypted` (`deployment.rs`), appelée à la fin de
      `cdep` après l'ajout du déploiement. Recalcule l'ensemble cible depuis
      `customers.yaml` (l'union des clients ayant réellement un **déploiement** sur cet
      hôte — pas `customer.hosts`, une déclaration plus lâche qui n'implique pas un accès
      secret réel), obtient le contenu en clair de la clé (déchiffre `identity_key`
      existant si déjà présent, sinon lit `identity_file` sur la machine locale la
      première fois), puis rechiffre pour l'ensemble complet. Best-effort et jamais
      fatal pour `cdep` : hôte sans clé lisible localement (ex. `localhost`) ignoré
      silencieusement, `identity_key` existant mais indéchiffrable avec les identités
      locales disponibles → avertissement, pas d'erreur bloquante.

      Câblage `ssh.rs` (consommation du champ, en même temps que ce qui le peuple pour
      la première fois) : `resolved_identity_path` préfère `identity_key` (déchiffré via
      `secrets::reveal`, matérialisé dans un fichier `0600` sous `~/.cache/ppo/keys/`,
      recréé à chaque appel — le déchiffrement `age` est de l'ordre de la microseconde)
      et se replie sur `identity_file` en cas d'absence ou d'échec de déchiffrement
      (jamais fatal pour la commande SSH en cours).

      Vérifié en live sur `mcm` (hôte réel, production) : client scratch `SSHKeyTest`
      lié à `mcm` via `cdep` → message `🔐 Clé SSH de 'mcm' chiffrée pour 2 client(s) :
      Mael, SSHKeyTest` (Multibikes, qui référence `mcm` seulement via `customer.hosts`
      avec `deployments: []`, exclu comme attendu) ; clés générées pour Mael et
      SSHKeyTest en `0600` ; déchiffrement avec **chacune** des deux identités
      indépendamment → contenu identique aux 387 octets réels de `/etc/ssh/mcm`.
      `closeall` puis `ppo dps` (aucune connexion maître existante, authentification
      forcément fraîche) → connexion SSH réelle réussie ; fichier matérialisé
      `~/.cache/ppo/keys/vps_mcm` en `0600`, contenu **octet-identique** à `/etc/ssh/mcm`.
      `hosts.yaml`/`customers.yaml`/`context.yaml` restaurés à l'identique après coup
      (`identity_key` de `mcm` remis à absent — la migration définitive attend 8.4,
      volontairement : ce test valide le mécanisme, il ne doit pas être celui qui chiffre
      `mcm` en prod comme effet de bord).
- [ ] **8.4** `[Claude]` `ppo secrets encrypt` : migration en place de la config réelle
      (génère les clés clients manquantes, chiffre `identity_file`→`identity_key` et
      `db_credentials.password`, réécrit les YAML).
      *Fait quand* : plus aucun secret en clair dans `PurposeOps-config/` ; retirer le
      fichier de clé d'un client fait échouer exactement (et seulement) ce qui dépend de
      lui (preuve que le cloisonnement par client fonctionne réellement).

      **Commande écrite et vérifiée, migration réelle pas encore lancée.**
      `cmd_secrets_encrypt` (`secrets.rs`) : parcourt tous les déploiements de
      `customers.yaml`, chiffre chaque `db_credentials.password` encore en clair (clé du
      client propriétaire, générée si besoin) ; parcourt tous les hôtes de `hosts.yaml`
      et réutilise `ensure_host_key_encrypted` (8.3) pour chacun — pas de code dupliqué
      entre le déclenchement automatique par `cdep` et cette passe de rattrapage
      manuelle. `ppo secrets encrypt` exposé en sous-commande (`Command::Secrets`, comme
      `Command::Backup`).

      Vérifié **en isolation** (les chemins de `config.rs` sont codés en dur, donc
      impossible de pointer la commande vers une config de test séparée) : hôte scratch
      `migtest` (fichier de clé jetable, contenu arbitraire) + client scratch
      `MigrationTest` avec un mot de passe **injecté directement en clair** dans
      `customers.yaml` (contournement délibéré de `cdep`, pour simuler des données
      pré-Phase-8 jamais passées par le chiffrement automatique). En lançant
      `ppo secrets encrypt` sur la config réelle, la commande a correctement trouvé et
      chiffré **les 3** mots de passe encore en clair qui existaient à ce moment
      (Mael, Sylvie, et le scratch injecté) et **3** clés d'hôte (`mcm`, `ngner`,
      `migtest`), avec les bons ensembles de destinataires (`ngner` → Cocotte *et*
      Sylvie, tous deux avec un déploiement dessus ; Cocotte incluse alors qu'elle n'a
      aucun mot de passe DB — cohérent, la clé SSH d'hôte suit les déploiements, pas les
      mots de passe). Comme la commande n'a pas de mode isolé, ce test a chiffré pour de
      vrai les secrets réels de Mael et Sylvie comme effet de bord non voulu de la
      vérification — **annulé aussitôt** (`hosts.yaml`/`customers.yaml` restaurés à
      l'identique depuis une sauvegarde prise avant le test, `~/.config/ppo/keys/` vidé).
      La commande est donc prouvée correcte mais volontairement pas encore lancée pour
      de bon sur la config réelle : chiffrer les vrais secrets de production est une
      action délibérée, pas un effet de bord d'un test — en attente d'un accord explicite
      avant de l'exécuter pour de vrai.

## Phase 9 — Provisioning end-to-end `[Claude]` — ✅ fait

Déployer un nouveau service en une commande. Réutilise SSH (3), Docker (4), CRUD (5).

- [x] **9.1** `[Claude]` Port du `templater` : rendu d'un template (`templates/<Service>/`)
      avec substitution de variables → `compose.yaml` local.
      *Fait quand* : compose rendu identique à celui du templater nu (Vaultwarden).

      **Fait** : `src/template.rs` (`generate_compose`, exposée en CLI via `ppo template
      render <service> <docker_service_name>`, équivalent du `g dc` nu — inexistant côté
      nu comme sous-commande de `ppo`). Séparation pure/impure comme `prompt.rs`
      (`format_prompt`/`get_prompt_context`) : `substitute`/`expand_networks`/
      `format_example` sont pures et testées unitairement, `generate_compose` orchestre
      la lecture disque + les prompts. Comportements du nu reproduits fidèlement, y
      compris deux qui ont l'air de bugs mais n'en sont pas puisqu'il s'agit de parité :
      `service_name`/`container_name` sont **toujours** dérivés de `docker_service_name`,
      jamais saisis, même quand `template.yml` déclare sa propre description/exemple
      pour `container_name` (cas réel de Vaultwarden) ; les champs `parent`/`type`/
      `required`/`validation`/`default_pattern` du schéma YAML existent mais ne sont
      jamais lus par `generate-compose` côté nu, donc pas repris non plus.

      **Bug réel trouvé et corrigé avant tout test live** : la première version utilisait
      une `BTreeMap` pour les variables, triée alphabétiquement par clé — cassant l'ordre
      de saisie pour les variables de même `level` (ex. `templates/Caddy/template.yml` :
      `http_port`/`https_port`/`admin_port` sont toutes `level: 2` ; nu les demande dans
      l'ordre du fichier grâce à un tri stable, `BTreeMap` les aurait redemandées dans
      l'ordre alphabétique des clés). Corrigé en remplaçant `BTreeMap` par
      `serde_yaml_ng::Mapping` (adossée à `IndexMap`, préserve l'ordre du fichier) +
      `Vec::sort_by_key` (stable) sur `level` — trouvé en écrivant le test de rendu Caddy
      avant de le lancer en live, pas après.

      Vérifié en live contre les templates réels du dépôt (`ppo template render`, piloté
      par `pexpect`) : Vaultwarden rendu correctement (`container_name` bien ignoré au
      profit de `docker_service_name`) ; Caddy rendu avec le bon ordre de prompts
      (`http_port → https_port → admin_port → config_volume_path → admin_bind →
      networks`, confirmant la correction ci-dessus) et la bonne expansion multi-réseaux.
      Tentative de comparaison croisée automatisée avec `templater.nu` via `pexpect`
      abandonnée : la session nu interactive reste bloquée dans ce pty (`reedline`
      envoie une requête de position curseur `ESC[6n` à laquelle ce terminal ne répond
      jamais) — limitation d'émulation de terminal, pas un doute sur le code. Confiance
      établie via la relecture ligne à ligne de `templater.nu` (portée dans les tests
      unitaires avec sortie calculée à la main, pas devinée) plutôt que via cette
      comparaison croisée en direct. 9 tests unitaires purs, `cargo test` : 55/56 verts.
- [x] **9.2** `[Claude]` `ppor provision` : rendu → création du dossier distant (SSH) →
      push du compose → `docker compose up -d` → enregistrement du déploiement dans
      `customers.yaml` (CRUD 5.3). Confirmation avant `up`.
      *Fait quand* : un service scratch déployé de bout en bout sur un VPS de test.

      **Fait** : `src/provision.rs`, `ppo provision`. Capacité entièrement nouvelle, sans
      équivalent nu même partiel (voir l'exploration en tête de fichier : `templater.nu`
      rend en local sans jamais rien pousser, `docker-compose-functions.nu` ne pilote que
      des piles déjà connues de `docker ps -a`, `deployment-manager/core.nu` enregistre
      des métadonnées sans toucher SSH/Docker — aucun des trois n'est câblé aux autres).
      Aucun mécanisme de transfert de fichier n'existe non plus dans le projet (pas de
      `scp`/`rsync`) : le compose rendu est encodé en base64 et écrit via une commande
      shell sur la connexion ControlMaster déjà en place (`echo '<b64>' | base64 -d >
      'chemin'`) plutôt que d'ouvrir une connexion de transfert séparée — cohérent avec
      le choix déjà fait en Phase 3 de tout faire passer par le socket multiplexé
      partagé. Une seule confirmation, couvrant tout (mkdir, envoi, `up`) — pas de
      deuxième confirmation juste avant `up` séparément. Pas de gestion des champs DB :
      les services actuellement templatés (Vaultwarden, Caddy) n'en ont pas ; `cdep`
      reste la voie pour un déploiement avec base de données. Réutilise
      `deployment::ensure_host_key_encrypted` (8.3) et `deployment_id_exists` (5.3,
      rendue `pub(crate)`) plutôt que de dupliquer cette logique.

      Vérifié en live sur un **VPS réel** (`ngner`, choisi précisément parce que
      `localhost` contournerait entièrement SSH et ne testerait pas le nouveau chemin de
      transfert de fichier) : client scratch `ProvisionTest` lié à `ngner`, template
      Vaultwarden rendu et provisionné sous le nom `ppo-provision-test` sur le réseau
      Docker externe déjà existant `caddy-network`. Après coup, vérifié sur l'hôte
      lui-même : conteneur réellement démarré (`docker ps` distant), fichier
      `docker-compose.yml` poussé **octet-identique** au rendu local (aucune corruption
      via l'aller-retour base64), `identity_key` de `ngner` correctement rechiffré pour
      les 3 clients y ayant désormais un déploiement (Cocotte, ProvisionTest, Sylvie).
      Nettoyage complet ensuite : `docker compose down` + suppression du répertoire
      distant (nécessitant un conteneur `alpine` root éphémère, les fichiers de données
      Vaultwarden appartenant à un UID différent de l'utilisateur SSH — même gotcha déjà
      documenté dans CLAUDE.md pour `docker cp`), `dc` du client scratch,
      `hosts.yaml`/`customers.yaml`/`context.yaml` restaurés à l'identique.
      `tests/integration_workflow.py` relancé après coup (aucune régression) suite aux
      changements de visibilité dans `deployment.rs`.

## Phase 10 — Monitoring de flotte `[Claude]` — ✅ fait

**Changement de direction par rapport au plan initial** : la version originale de cette
phase prévoyait un `ppor fleet status` maison (agrégation SSH parallèle de `docker ps`/
`uptime`/`df` en tableau). Décision prise en discussion : ne pas réinventer un système de
monitoring — l'historique, l'alerting et les dashboards de Netdata (agent par hôte,
auto-découverte des conteneurs Docker locaux, installé **directement sur l'hôte**, pas en
conteneur, pour cette auto-découverte) couvrent ce besoin bien mieux qu'un snapshot à la
demande. `10.1`/`10.2` (`fleet status`/`fleet logs`) sont donc abandonnées au profit d'un
module de bootstrap qui installe Netdata (et les autres logiciels de base) sur un hôte.

- [x] **10.1** `[Claude]` `ppo bootstrap [host_id]` : installation des logiciels de base
      d'un hôte (Docker, Nushell, Caddy, Netdata) — capacité entièrement nouvelle, aucun
      équivalent côté nu.
      *Fait quand* : logiciels manquants détectés et installés sur un hôte réel sans
      toucher à ce qui est déjà présent.

      **Fait** : `src/bootstrap.rs`. Module séparé plutôt qu'ajout à `provision.rs` : une
      liste de capacités fixes (Docker, Nushell, Caddy, Netdata), définie par l'opérateur
      dans le code, pas par un wizard d'édition à chaud comme `services.yaml` — mérite un
      fichier à part du reste de la CRUD config, à la demande explicite du sujet. Chaque
      capacité a une commande de détection (`command -v ...`) et une commande
      d'installation, écrites pour un hôte Debian/Ubuntu (apt) — la flotte actuelle n'a
      pas besoin de détection de distribution. Aucun état "installé" mis en cache dans
      `hosts.yaml` : chaque run revérifie en direct (`is_installed`), bon marché à cette
      échelle et évite toute dérive entre config et réalité de l'hôte. Détection en
      premier pour tous les hôtes, puis sélection multiple (`inquire::MultiSelect`,
      nouveau dans `ui.rs`) limitée à ce qui manque, une seule confirmation, puis
      installation capacité par capacité.

      Docker : script officiel `get.docker.com` (gère lui-même l'élévation `sudo` si
      besoin). Netdata : `kickstart.sh --non-interactive` (idem), installé sur l'hôte et
      non en conteneur pour que l'auto-découverte des conteneurs Docker locaux fonctionne.
      Caddy : dépôt apt officiel (clé GPG + `sources.list.d`) puis `apt-get install`.
      Nushell : pas de dépôt apt officiel, binaire de la dernière release GitHub
      téléchargé et installé dans `/usr/local/bin`.

      Extraction de `ssh::exec_shell`/`exec_shell_checked` (gère le cas `localhost` en
      plus de `run_with_master`) depuis l'ancien `exec_remote_shell_checked` privé de
      `provision.rs`, pour que les deux modules partagent la même primitive plutôt que de
      dupliquer la logique locale/distante — `provision.rs` mis à jour pour l'utiliser.

      Partie pure testée unitairement (`missing_capabilities`, injectée par une closure de
      présence plutôt que de dépendre de SSH) : unicité des labels, commandes non vides,
      filtrage correct. 5 tests, `cargo test` : 60/61 verts (le test SSH live existant
      reste `#[ignore]`).

      Vérifié en live contre l'hôte réel `ngner` (celui déjà utilisé en 9.2) : détection
      correcte de l'état réel (Docker/Nushell/Netdata déjà présents, Caddy absent),
      annulation propre de la sélection multiple (Échap) sans erreur. Installation
      effective **volontairement pas testée en direct sur `ngner`** : c'est un hôte de
      production avec du trafic client réel, et y ajouter Caddy sort du cadre d'une
      vérification — décision laissée à l'opérateur plutôt que prise unilatéralement
      pendant la vérification.
- [x] **10.2** `[Claude]` Harnais de test dédié : une VM VirtualBox plutôt qu'un conteneur
      Docker (Docker-in-Docker et systemd-in-conteneur sont tous deux des sources connues
      de faux échecs sans rapport avec `bootstrap.rs`, et la flotte réelle est faite de VPS
      complets, pas de conteneurs), pour pouvoir vérifier une installation *effective* sans
      se contenter de la détection contre `ngner` ci-dessus.
      *Fait quand* : cycle complet install → vérification fonctionnelle → ré-installation
      idempotente, rejouable sans réintervention manuelle.

      **Fait** : `tests/vm/` (`setup.sh`, `vmctl.sh`, `lib.sh`) + `tests/bootstrap_workflow.py`.
      `setup.sh` provisionne une fois (image cloud Ubuntu 24.04 — la distribution réelle du
      poste local et de la prod, cf. décision utilisateur ci-dessus — convertie en VDI,
      cloud-init pour l'utilisateur `ppo` avec sudo NOPASSWD et une clé SSH dédiée, NAT +
      port forwardé pour l'accès SSH) puis prend un snapshot `clean` une fois le
      provisionnement terminé. `bootstrap_workflow.py` (même patron pexpect
      qu'`integration_workflow.py`) restaure ce snapshot et redémarre la VM à chaque run —
      c'est ce qui rend le test rejouable sans re-télécharger/re-provisionner à chaque fois.
      Orchestration VirtualBox (`VBoxManage`) confinée à `vmctl.sh` en shell ; Python reste
      concentré sur le pilotage de `ppo` via `pexpect`, même séparation des responsabilités
      qu'ailleurs dans le projet.

      **Deux bugs réels trouvés et corrigés en construisant ce harnais** (ni l'un ni
      l'autre dans `bootstrap.rs` — tous deux dans `setup.sh`, découverts uniquement parce
      que le scénario "redémarrer depuis un snapshot" n'avait jamais été exercé avant) :
      1. `ssh.service` échouait systématiquement (`ExecStartPre=/usr/sbin/sshd -t` en
         échec) sur tout redémarrage **après** le premier — jamais sur le tout premier
         boot. Cause : `setup.sh` faisait un `VBoxManage controlvm poweroff` (arrêt dur)
         immédiatement après la fin de `cloud-init status --wait`, sans laisser le temps
         aux clés hôte SSH fraîchement régénérées par cloud-init d'être effectivement
         synchronisées sur disque avant que le snapshot ne fige cet état — un
         redémarrage ultérieur récupère alors des fichiers de clé incomplets/incohérents.
         Diagnostiqué en basculant le port série de la VM en mode socket interactif
         (`--uartmode1 server`, plutôt que le `file` habituel) et en s'y connectant via
         `socat` piloté par un script `pexpect` jetable (une console série ne répond pas
         au réseau/SSH, donc aucun autre moyen d'atteindre une VM dont SSH est justement
         cassé) pour lire `systemctl status`/`journalctl` directement. Corrigé en
         remplaçant l'arrêt dur par `sudo shutdown -h now` côté invité et une attente
         active de l'état `poweroff` avant de prendre le snapshot (repli sur l'arrêt dur
         si ça dépasse 120s).
      2. `VBoxManage modifyvm` (p. ex. changer `--uartmode1` pour le diagnostic ci-dessus)
         appliqué **avant** un `VBoxManage snapshot restore` se voit annulé par la
         restauration — les snapshots VirtualBox capturent aussi les réglages machine, pas
         seulement l'état disque. Il faut appliquer ce genre de changement après le
         restore, pas avant.

      Cycle complet vérifié à plusieurs reprises : `ch` (hôte pointant sur
      `127.0.0.1:<port forwardé>`, délibérément pas `localhost`, pour emprunter le vrai
      chemin ControlMaster) → `ppo bootstrap` sur VM fraîche (les 4 capacités détectées
      manquantes, sélection totale via le raccourci "→" de `MultiSelect`, installation) →
      vérification fonctionnelle **en direct par SSH, indépendamment de `ppo`** (`docker
      run --rm hello-world` réussit, pas seulement `docker --version` ; `nu`/`caddy`
      répondent ; l'API locale de Netdata répond, avec ré-essais courts car elle renvoie
      503 le temps de son premier cycle de collecte) → deuxième `ppo bootstrap` sur la même
      VM, confirmant qu'aucune capacité n'est réinstallée → `dh`. `hosts.yaml`/
      `context.yaml` restaurés à l'identique après coup, comme `integration_workflow.py`.

## Réorganisation du dépôt : Rust à la racine, module nu archivé

Faite juste avant de fusionner la branche `rust` dans `master`. Motivation concrète : les
statistiques « Languages » de GitHub reflètent l'arbre de la branche **par défaut**
(`master`), pas de la branche courante — tout le travail Rust ayant vécu sur `rust` sans
jamais être fusionné, `master` ne montrait toujours que le nu, d'où le 100% Nushell affiché
sur la page du dépôt alors que le portage Rust était déjà bien avancé (démarré le
2026-07-10). La fusion à elle seule aurait réglé
l'affichage ; réorganiser l'arborescence en même temps aligne aussi la structure du dépôt
avec ce qui est réellement le projet actif.

- Contenu de `ppo-rs/` (`Cargo.toml`, `Cargo.lock`, `src/`, `tests/`) remonté à la racine du
  dépôt via `git mv` (historique de fichier préservé). `ppo-rs/` supprimé.
- Code nu (`ppo.nu`, `config/`, `context/`, `customer-manager/`, `deployment-manager/`,
  `docker/`, `docker-compose-functions.nu`, `machine-manager/`, `service-manager.nu`,
  `ssh-manager.nu`, `templater.nu`) déplacé sous `archive/` via `git mv`. Une seule
  référence à un chemin absolu à corriger après coup : `ppo.nu`'s `ppos` (palette fzf)
  recharge le module par chemin en dur pour l'exécution directe d'une commande —
  `~/dev/nu-modules/PurposeOps/ppo.nu` → `~/dev/nu-modules/PurposeOps/archive/ppo.nu`.
  Aucune autre référence de chemin absolu trouvée dans les fichiers nu (vérifié par grep
  avant le déplacement) : tous les `use` inter-fichiers sont relatifs, donc intacts
  puisque l'arborescence entière a été déplacée en bloc.
- `PurposeOps-config/` (submodule) et `templates/` **ne bougent pas** : `templates/` est
  lue par `template.rs` via `config::base_path()` (un chemin absolu fixe côté `$HOME`,
  indépendant de l'emplacement du code dans le dépôt), donc partagée entre le binaire Rust
  et l'ancien module nu, pas seulement de la donnée nu.
- `.gitignore`, `.github/workflows/ci.yml` (plus de `working-directory: ppo-rs`),
  `README.md` et `CLAUDE.md` mis à jour pour les nouveaux chemins. Entrées `config/*.yaml`
  historiquement mortes dans `.gitignore` (déjà signalées comme telles dans `CLAUDE.md`)
  supprimées à cette occasion, `config/` n'existant même plus à cet endroit.
- Aucun changement de comportement à l'exécution : les chemins codés en dur dans le
  binaire (`config::base_path()`, `controlmasters/`, la clé SSH matérialisée sous
  `~/.cache/ppo/keys/`) sont ancrés sur `$HOME`, pas sur l'emplacement de `Cargo.toml`
  dans le dépôt — aucune modification de `src/` nécessaire pour ce déplacement.
  Revérifié après coup : `cargo build`/`cargo test`/`cargo clippy` verts depuis la racine.

## Phase 11 — Agent de backup autonome `[Claude]` — ✅ fait (11.6 volontairement différé)

**Changement de direction par rapport au plan initial.** La version originale de cette
phase prévoyait un `ppor backup all` appelé par le timer système **du laptop**. Problème
concret soulevé en discussion : ça fait dépendre les sauvegardes de la disponibilité du
laptop (allumé, éveillé, sur le réseau) au moment où le cron doit se déclencher — pas
acceptable pour des sauvegardes nocturnes non surveillées. Nouvelle conception : chaque
déploiement reçoit un agent `ppo` autonome poussé sur **son propre hôte**, et c'est le
cron de CET hôte qui lance `ppo backup run --cron` localement, avec alerte `ntfy` en cas
d'échec.

Constat clé qui rend ça possible sans dupliquer la logique de sauvegarde : chaque point de
dispatch distant (`docker::run_docker_command`, `backup.rs`'s `exec_remote_shell`,
`ssh::exec_shell`) vérifie déjà `host.hostname == "localhost"` en premier et prend un
chemin d'exécution locale pure avant de toucher à SSH/ControlMaster. Un `Host` scopé avec
`hostname: "localhost"` suffit donc à faire tourner `do_generic_backup`/`run_backup_steps`
**tels quels**, sur l'hôte lui-même — zéro changement à la logique de sauvegarde, zéro
duplication (pas de script bash généré séparément : l'agent, c'est le vrai binaire `ppo`).

- [x] **11.1** `[Claude]` Rotation/rétention : `--keep-last N` sur `backup run`, purge des
      archives au-delà des N plus récentes dans le dossier de sortie.
      *Fait quand* : N backups gardés, plus vieux purgés automatiquement.

      **Fait** : `src/backup.rs`, `backups_to_purge` (pure, testée) + `purge_old_backups`
      (réutilise `list_remote_backups`, déjà triée `ls -1t` du plus récent au plus
      ancien). Pas de valeur par défaut cachée dans `backup run` lui-même : sans
      `--keep-last`, aucune purge — le "10 par défaut" ne vit que dans le flag de
      `bootstrap-agent` (11.4), pour que le comportement de `backup run` reste entièrement
      prévisible depuis ses propres arguments, peu importe qui l'appelle. Purge
      best-effort/non-fatale : un échec de purge n'invalide pas un backup par ailleurs
      réussi et ne déclenche pas de notification `ntfy` (le backup, lui, a réussi).

- [x] **11.2** `[Claude]` Transfert d'un exécutable compilé vers un hôte distant :
      `provision::push_binary`, envoi par blocs plutôt qu'en une seule commande.
      *Fait quand* : un binaire de plusieurs Mo arrive intact sur l'hôte distant.

      **Fait** : `src/provision.rs`. Le mécanisme existant (`push_file`, `echo '<b64>' |
      base64 -d > path` en une seule commande shell) est un **blocage réel, mesuré**, pas
      une hypothèse : Linux plafonne un unique argument/variable d'environnement à
      `MAX_ARG_STRLEN` = 128 KiB (indépendant du plus grand `ARG_MAX` = 2 MiB qui limite
      la somme argv+envp — confirmé `getconf ARG_MAX` = 2097152 sur cette machine), et le
      binaire release fait ~4.8 Mo une fois strippé (`[profile.release] strip = true`
      ajouté à `Cargo.toml`, 6.1 Mo → 4.8 Mo mesuré), soit largement plus une fois encodé
      en base64. `push_binary` découpe donc en blocs de 64 KiB bruts (~87 KiB encodés,
      marge confortable sous 128 KiB), écrits en `>>` successifs sur la connexion
      ControlMaster déjà authentifiée, purge un envoi précédent interrompu avant de
      commencer (les blocs s'accumulent, un fichier partiel corromprait le résultat),
      vérifie l'intégrité par `sha256sum` (local + distant, comparés) plutôt que d'ajouter
      une dépendance `sha2` — même principe que `local_timestamp()` s'appuyant sur `date`.
      **La taille de bloc de 64 KiB est une valeur de départ dérivée du calcul de la
      contrainte, pas encore validée en conditions réelles contre un VPS distant** — à
      confirmer lors de la vérification live (11.7).

- [x] **11.3** `[Claude]` Identité `age` scopée à l'agent, pas celle du client.
      *Fait quand* : le mot de passe DB poussé sur l'hôte se déchiffre avec une identité
      qui ne déchiffre rien d'autre.

      **Fait** : `src/secrets.rs`, `load_or_generate_agent_identity`/
      `agent_identity_path` (`~/.config/ppo/keys/agent-<deployment_id>.txt`), même
      mécanique que `load_or_generate_customer_identity`. **Décision de sécurité
      explicite** : l'identité RÉELLE d'un client déchiffre tous ses secrets, sur tout le
      parc, pour toujours ; la mettre sur un hôte de cron non surveillé — hôte souvent
      partagé entre plusieurs clients (`ngner` héberge à la fois Cocotte et Sylvie) — ferait
      qu'un seul hôte compromis expose tout l'historique de secrets de ce client, pas
      seulement ce mot de passe DB. À la place, une identité par déploiement est ajoutée
      comme second destinataire aux côtés de celle du client (même mécanisme
      multi-destinataires `age` déjà utilisé pour `Host.identity_key` en Phase 8.3) —
      `backup_agent::ensure_agent_recipient` fait ce (ré)chiffrement et réécrit le
      `customers.yaml` réel. **Même catégorie d'action que la migration Phase 8.4** (mute
      des secrets réels) : à vérifier contre des données de test avant tout déploiement
      contre un client réel — pas encore fait à ce stade (voir 11.7).
      `load_all_local_identities` n'a nécessité aucune modification : elle charge déjà
      tout fichier `.txt` de `~/.config/ppo/keys/` quel que soit son nom, donc `reveal()`
      prend en compte une identité d'agent dès qu'elle est présente sur la machine.

- [x] **11.4** `[Claude]` `src/backup_agent.rs` (nouveau module) + `ppo backup
      bootstrap-agent [deployment_id] [--ntfy-url URL] [--keep-last N]`.
      *Fait quand* : un déploiement scratch tourne en autonome sur son propre hôte.

      **Fait**. Module séparé plutôt qu'ajout à `bootstrap.rs`/`backup.rs`/`provision.rs` :
      `bootstrap.rs` est conçu pour une liste fixe de capacités **au niveau de l'hôte**,
      sans notion de "quel déploiement" (voir Phase 10.1) — mauvaise forme pour ce
      besoin ; `backup.rs` exécute une sauvegarde, ce module-ci *installe ce qui exécutera
      plus tard des sauvegardes* — cycle de vie différent. Réutilise les primitives de
      transfert de `provision.rs` (`push_file` rendu `pub(crate)`, `push_binary`).

      Builders purs et testés (`src/backup_agent/tests.rs`) : `build_scoped_host`
      (`hostname` forcé à `localhost`, `identity_key`/`identity_file`/`port` vidés — un
      `Host` nommé `localhost` n'atteint jamais la résolution d'identité SSH, inutile d'y
      dupliquer la clé de la VPS elle-même), `build_scoped_customers` (un seul client, un
      seul déploiement — celui ciblé, même si ce client en a d'autres ailleurs),
      `build_scoped_context`, `build_cron_line`.

      **Bug réel trouvé en écrivant le test live (11.7), avant même de le lancer** :
      `docker::run_docker_command` ne préfixe jamais `sudo` côté distant — hypothèse vraie
      aujourd'hui uniquement parce que chaque hôte existant a été provisionné à la main
      avant `ppo`, avec l'utilisateur SSH déjà dans le groupe `docker`. Un hôte fraîchement
      préparé par `ppo bootstrap` (Phase 10) n'a PAS cette appartenance : le script
      `get.docker.com` se contente de *suggérer* `usermod -aG docker`, il ne le fait pas.
      Sans correctif, l'agent (ses propres appels `docker`, jamais en `sudo`) aurait
      échoué en "permission denied" à chaque exécution cron réelle. Corrigé en ajoutant
      `sudo usermod -aG docker <user>` (idempotent) tôt dans `cmd_backup_bootstrap_agent` —
      inutile pour les étapes de LA commande elle-même (aucune n'appelle `docker`
      directement), mais l'appartenance à un groupe ne prend effet qu'à une nouvelle
      session de connexion, et c'est exactement ce que cron démarre à chaque déclenchement.

      Orchestration (`cmd_backup_bootstrap_agent`) : résolution du déploiement (menu sur
      TOUS les clients si omis — outil de mise en place ponctuelle, ne dépend pas du
      contexte de session courant) → ajout au groupe `docker` (ci-dessus) →
      `ensure_agent_recipient` (11.3) → vérification d'architecture (`Host.arch` vs
      `std::env::consts::ARCH`, mappage `arm64`↔`aarch64` ; erreur explicite et
      actionnable si différente — voir 11.6, pas de compilation croisée automatisée) →
      `cargo build --release` → envoi de `hosts.yaml`/
      `customers.yaml`/`context.yaml` scopés, de l'identité d'agent (`chmod 600`), du
      binaire (`push_binary`) → **test de fumée obligatoire** (`<binaire> --help` en SSH,
      immédiatement après l'envoi — gratuit grâce à `clap`, attrape mauvaise architecture/
      bibliothèque partagée manquante/transfert corrompu tout de suite plutôt que
      silencieusement au premier cron réel) → rendu + installation de la tâche cron
      (`sudo install -m 0644 ... /etc/cron.d/ppo-backup-<deployment_id>`, réécriture
      atomique, donc idempotente par construction).

      Convention de chemin réutilisée telle quelle sur l'hôte distant
      (`~/dev/nu-modules/PurposeOps/PurposeOps-config/...`), **pas de nouveau flag
      `--config-dir`** : vérifié que tout ce qui vit sous `config::base_path()` et
      dépendrait spécifiquement du laptop (`controlmasters/`, `~/.cache/ppo/keys/` pour la
      clé SSH matérialisée) n'est atteint que par `run_with_master`/
      `resolved_identity_path`, structurellement inatteignables une fois
      `hostname == "localhost"`. Réutilisation aussi de `ssh::resolve_remote_path`
      (promue depuis `backup.rs`, désormais partagée par les deux modules) pour les
      chemins `~/...` construits pour l'agent — les entourer de guillemets simples dans
      une commande shell distante en empêcherait sinon l'expansion (gotcha déjà documenté
      dans CLAUDE.md).

      **Deuxième bug réel trouvé en écrivant le test live (11.7)** : `resolve_remote_path`
      remplaçait `~` par `/home/ngner` codé en dur — vrai partout sur le parc réel
      (utilisateur SSH toujours `ngner`), faux contre la VM de test (utilisateur `ppo`),
      ce qui aurait fait pousser la config/le binaire/l'identité au mauvais endroit
      (`/home/ngner/...`, ni accessible en écriture ni même existant pour l'utilisateur
      `ppo`). Corrigé en dérivant le home de `host.user` (`/home/<user>`) plutôt qu'une
      constante — signature de `resolve_remote_path` changée en conséquence, tous les
      appels de `backup.rs` (5) et `backup_agent.rs` (4) mis à jour pour passer
      `host.user`/`real_host.user`. Aucun changement de comportement sur le parc réel
      (tous les hôtes actuels ont `user: ngner`), mais la fonction ne dépend plus d'une
      hypothèse fausse dès qu'un hôte a un utilisateur différent — exactement le genre de
      chose que la Phase 9.2/10.2 n'avaient jamais eu l'occasion de révéler, faute d'avoir
      testé contre un hôte à utilisateur SSH différent avant celui-ci.

- [x] **11.5** `[Claude]` Notification d'échec via `ntfy`.
      *Fait quand* : un backup cron en échec déclenche une notification `ntfy` réelle.

      **Fait** : `src/backup.rs`, `notify_failure`, gérée **à l'intérieur de
      `cmd_backup_run`** plutôt qu'en `curl || ...` sur la ligne cron — `Result` porte
      déjà l'étape précise qui a échoué (`anyhow::Error`), un `||` au niveau shell après
      la sortie du processus n'aurait su qu'un "code de sortie non nul", perdant ce
      détail. `cmd_backup_run` est désormais une fine enveloppe autour de
      `run_backup_for_current_deployment` : elle couvre aussi les erreurs de pré-vol
      (client/déploiement introuvable, credentials illisibles...), pas seulement celles
      internes à `do_generic_backup` — sur un agent scopé à un seul déploiement, n'importe
      quelle erreur de ce chemin mérite une alerte. Topic/URL transitent par une ligne
      `NTFY_URL=...` dans le fichier `/etc/cron.d/...` généré (`cron.d` accepte des
      affectations de variable d'environnement) ; lue au runtime, absente = notification
      silencieusement sautée (un run manuel interactif sur le laptop n'a pas besoin d'une
      alerte push) — zéro changement de schéma YAML. **Angle mort accepté explicitement** :
      si le binaire ne démarre même pas (transfert corrompu, bibliothèque manquante),
      aucune notification interne ne se déclenche — partiellement couvert par le test de
      fumée `--help` obligatoire de 11.4, qui attrape cette classe de panne *avant*
      l'installation de la tâche cron plutôt que de la découvrir silencieusement au
      premier déclenchement réel.

      **Vérifié contre la vraie instance `ntfy` du sujet** (self-hosted sur `mcm`,
      `ntfy.ngner.space`, découverte en lisant le `Caddyfile` en direct), pas seulement
      `ntfy.sh` (utilisé pendant 11.7 pour le test VM, ouvert par défaut). Un premier essai
      a échoué en `403` : contrairement à `ntfy.sh`, cette instance a
      `auth-default-access: "deny-all"` (`/home/ngner/ntfy/config/server.yml`) — toute
      publication non authentifiée est refusée. `notify_failure` fait un simple `curl -d
      ... <url>` sans authentification : **tel quel, il aurait échoué en silence contre
      une instance auto-hébergée protégée**, pas seulement contre `ntfy.sh`. Pas un bug de
      code à proprement parler (aucun changement nécessaire dans `backup.rs`) mais une
      hypothèse d'usage à documenter : `curl`/`--ntfy-url` supportent nativement les
      identifiants HTTP Basic embarqués dans l'URL (`https://user:pass@host/topic`), donc
      la solution est de fournir `--ntfy-url` sous cette forme pour une instance protégée,
      pas de changer le code. Utilisateur `ppo-agent` créé sur l'instance réelle du sujet
      (`docker exec ntfy ntfy user add`, accès `write-only` limité au seul topic
      `homelab` via `ntfy access` — jamais admin, jamais accès aux autres topics),
      confirmé fonctionnel par un envoi réel reçu sur l'appareil du sujet.

- [ ] **11.6** `[Claude]` Compilation croisée (`Host.arch` différent de la machine qui
      lance `bootstrap-agent`) — **spec seulement, pas d'automatisation**.
      *Fait quand* : un hôte `arm64` réel existe et peut recevoir un agent compilé pour
      lui.

      Décision assumée : tout hôte du parc actuel est `x86_64` (vérifié dans
      `hosts.yaml`) ; construire et tester une chaîne de compilation croisée contre zéro
      hôte `arm64` réel serait spéculatif. `backup_agent::host_matches_local_arch` détecte
      déjà le cas et échoue avec un message actionnable (`rustup target add <triple>` +
      paquet linker croisé apt, ex. `gcc-aarch64-linux-gnu`, puis `cargo build --release
      --target <triple>`) plutôt que de tenter quoi que ce soit automatiquement. À
      reprendre quand un hôte non-`x86_64` existera réellement — recommandation pour ce
      moment-là : cibles `*-unknown-linux-musl` plutôt que `gnu`, pour éviter un décalage
      de version glibc entre la machine de build et le Debian/Ubuntu réel de la VPS.

- [x] **11.7** `[Claude]` Vérification live (VM, comme la Phase 10.2).
      *Fait quand* : cycle complet installation → vérification fonctionnelle →
      ré-exécution idempotente → notification d'échec réelle, rejouable sans
      réintervention manuelle.

      **Fait** : `tests/backup_agent_workflow.py`, réutilisant `tests/vm/` (déjà construit
      pour la Phase 10.2) plutôt qu'un second harnais. Revert/boot de la VM → `ch` (hôte
      distinct de `localhost`, pour emprunter le vrai chemin ControlMaster côté envoi) →
      `ppo bootstrap` (Docker seul, pas les 4 capacités — inutile ici) → conteneurs
      postgres + "app" factice minimal démarrés en direct par SSH (la branche "filestore
      absent" de `run_backup_steps` gère déjà l'absence de filestore, pas besoin d'un vrai
      Odoo) → `cdep` avec de vrais `db_credentials` → `ppo backup bootstrap-agent
      --ntfy-url https://ntfy.sh/<topic-de-test-unique>` depuis le laptop → vérification
      **en direct par SSH, indépendamment de `ppo`** (binaire `--help`, contenu du
      `/etc/cron.d/...`, YAML scopés, permissions `600` de l'identité d'agent) →
      déclenchement direct de `backup run --cron` par une connexion SSH ponctuelle
      (nouvelle session à chaque fois, comme le ferait cron réellement — pas la connexion
      ControlMaster persistante de `ppo`, pour que l'effet du correctif groupe `docker`
      ci-dessus soit vraiment exercé) → archive confirmée sur le disque distant → casse
      volontaire (arrêt du conteneur DB) + nouveau déclenchement → code de sortie non nul
      **et** sondage réel de l'API `ntfy` (`.../json?poll=1`) confirmant que la
      notification est réellement arrivée → ré-exécution de `bootstrap-agent`, fichier
      `cron.d` remplacé et non dupliqué → nettoyage (`ddep`/`dc`/`dh`, restauration du
      snapshot de config, arrêt de la VM). **Passé du premier coup, une fois les deux bugs
      ci-dessous corrigés.**

      **Deux bugs réels supplémentaires trouvés en écrivant/lançant ce test** (aucun dans
      la logique de sauvegarde elle-même) :
      1. La réorganisation du dépôt (voir plus haut, "Rust à la racine") avait cassé le
         calcul de `CONFIG_DIR` dans `integration_workflow.py`/`bootstrap_workflow.py` —
         `os.path.dirname(REPO_ROOT)` pointait vers le mauvais dossier une fois `tests/`
         remonté d'un niveau (avant, `REPO_ROOT` = `ppo-rs/`, donc `dirname(REPO_ROOT)` =
         racine du dépôt ; après, `REPO_ROOT` = racine du dépôt directement, donc
         `dirname(REPO_ROOT)` = *son parent*). Personne ne l'avait remarqué : aucun des
         deux scripts n'avait été relancé après la restructuration, seuls `cargo build`/
         `test`/`clippy` et `nu-check` l'avaient été. Corrigé (`CONFIG_DIR =
         os.path.join(REPO_ROOT, "PurposeOps-config")`), et `integration_workflow.py`
         relancé avec succès pour confirmer.
      2. La VM `ppo-bootstrap-test` elle-même avait survécu à la restructuration avec un
         disque enregistré sur l'ancien chemin (`ppo-rs/tests/vm/ubuntu-24.04.vdi`) —
         `git mv` déplace aussi les fichiers non suivis d'un répertoire entier (les
         `.vdi`/`.img` sont gitignorés mais ont bien été déplacés physiquement), mais
         VirtualBox ne relocalise jamais un médium enregistré tout seul. Un `unregistervm
         --delete` classique échouait aussi (même chemin cassé) ; nettoyage manuel via
         `VBoxManage closemedium` sur le disque parent et son différentiel de snapshot,
         puis `tests/vm/setup.sh --recreate` (rapide : réutilise l'image cloud déjà
         téléchargée).

## Nouveau template : Odoo `[Claude]`

Hors plan de phases — ajout d'un troisième template (`templates/Odoo/`, enregistré dans
`services.yaml`) aux côtés de Vaultwarden/Caddy, à la demande explicite du sujet, à partir
du déploiement réel `~/odoo-perso` sur l'hôte `mcm` (inspecté en direct par SSH : son
`docker-compose.yml`, `Dockerfile.prod`, `entrypoint.sh`, `odoo.conf`).

**Décision de conception** : `odoo-perso` construit son image localement, mais son
`Dockerfile.prod` s'avère être quasiment l'image officielle Odoo (le Dockerfile publié par
Odoo S.A.) reconstruite à la main, sans rien d'ajouté que l'image publique `odoo:18.0`
n'ait déjà. Décidé en discussion : le template pousse `odoo:18.0` directement plutôt que de
répliquer un contexte de build complet (Dockerfile + entrypoint + config) — cohérent avec
Vaultwarden/Caddy qui ne font eux non plus que tirer une image publique, et évite d'étendre
`provision.rs` pour pousser autre chose qu'un seul `docker-compose.yml` rendu.

Deux services dans un seul template (`{{service_name}}` app + `{{db_service_name}}`
Postgres) : premier template du projet à en avoir besoin — `service_name`/`container_name`
restent auto-remplis depuis `docker_service_name` comme pour Vaultwarden/Caddy (voir
`template.rs`), mais le service Postgres a besoin de son propre nom, saisi une fois et
réutilisé pour sa clé de service ET son `container_name`. Réseau interne dédié
(`{{service_name}}-internal`, dérivé par substitution littérale sans variable
supplémentaire — même technique que le réseau externe de Caddy) pour que Postgres ne soit
joignable que par l'app, pas exposé sur le réseau du reverse proxy.

**Bug réel trouvé en vérifiant le template en direct** (`docker compose up` réel, pas
seulement `docker compose config`) : la première version montait `/var/lib/odoo` et
`/var/lib/postgresql/data` sur des chemins hôte (bind mounts), au même chemin que
`custom-addons`/`config`. `odoo:18.0` tourne en UID 100/GID 101 (`odoo`) ; un dossier hôte
fraîchement créé appartient à l'utilisateur courant, pas à cet UID — `PermissionError:
/var/lib/odoo/.local` au démarrage, `/web/login` répondant en 500. Même classe de bug que
l'incident filestore de la Phase 6 (déjà documenté dans CLAUDE.md). Corrigé en remplaçant
ces deux montages par des **volumes Docker nommés** (`{{service_name}}-data`/
`{{service_name}}-db-data`) plutôt que des bind mounts : un volume nommé est initialisé par
Docker avec les bonnes permissions dès la première écriture du conteneur, éliminant la
classe de bug entièrement plutôt que de documenter un `chown` à faire à la main à chaque
déploiement. `addons_volume_path`/`config_volume_path` restent des bind mounts
volontairement (l'opérateur doit pouvoir éditer les addons et `odoo.conf` directement sur
l'hôte — même raisonnement que le montage de config de Caddy).

Vérifié en direct à deux reprises (avant et après le correctif) : pile complète démarrée en
local via `docker compose up` avec les valeurs rendues par `ppo template render Odoo
<nom>`, logs Odoo confirmant la connexion Postgres réussie via les variables d'environnement
`HOST`/`USER`/`PASSWORD`, `curl` depuis un conteneur tiers sur le même réseau confirmant un
`200` sur `/web/login` (après redirection, normal sans base de données encore créée) une
fois le correctif appliqué — `500` avant. `ppo check` et `ppo lss` confirmés après ajout à
`services.yaml`. Pas de changement à `provision.rs`/`template.rs` : le template respecte le
mécanisme existant tel quel.

Non couvert par ce changement, comme documenté pour les autres templates : `ppo provision`
ne gère pas les champs `db_credentials` (voir sa doc dans `provision.rs`) — un déploiement
Odoo provisionné via `ppo provision` n'aura pas de sauvegarde configurée automatiquement,
`cdep` reste la voie pour enregistrer les champs DB après coup si une sauvegarde est
nécessaire.

## Phase 12 — TUI `[Claude, gros morceau]`

Surcouche `ratatui` sur le socle CLI (le CLI reste la référence scriptable).

- [ ] **12.1** `ppor tui` : navigation hôte → client → déploiement, sélection visuelle
      qui écrit le contexte.
      *Fait quand* : changer de contexte sans taper de commande.
- [ ] **12.2** Actions depuis la TUI : status flotte, start/stop conteneur, lancer un
      backup, avec confirmations.
      *Fait quand* : les opérations courantes se font sans quitter la TUI.
