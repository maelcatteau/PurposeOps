# PurposeOps

`PurposeOps` (alias CLI `ppo`) est un module [Nushell](https://www.nushell.sh/) personnel pour piloter un
petit parc de déploiements clients hébergés dans Docker (principalement des instances Odoo) sur
plusieurs hôtes VPS distants, via SSH.

Pas de build, pas de suite de tests, pas de CI : c'est un ensemble de modules Nushell chargés
directement dans un shell interactif, sur le modèle d'un `kubectl` maison — avec une notion de
"contexte courant" (host / customer / deployment sélectionnés).

## Pré-requis

- [Nushell](https://www.nushell.sh/)
- Un accès SSH configuré vers les hôtes distants gérés
- `docker` installé sur les hôtes cibles
- (optionnel) [`fzf`](https://github.com/junegunn/fzf) pour le sélecteur de commandes interactif `ppos`

## Installation

`PurposeOps-config` est un submodule git séparé (dépôt privé) qui contient les données réelles (hosts,
customers, deployments, credentials, services). Il faut le cloner avec :

```nu
git clone --recurse-submodules https://github.com/maelcatteau/PurposeOps.git
```

Puis charger le module dans la config Nushell (`~/.config/nushell/config.nu`) :

```nu
use ~/dev/nu-modules/PurposeOps/ppo.nu
```

Toutes les commandes sont alors disponibles sous forme d'alias courts (`ppo` n'est pas un préfixe de
commande, ce sont les alias eux-mêmes qui sont chargés dans le shell, ex: `sc`, `sd`, `dps`...).

⚠️ Nushell parse `use` au moment où il s'exécute : éditer un fichier `.nu` ne recharge pas à chaud un
shell déjà lancé. Il faut redémarrer le shell (ou re-`use` le module) pour prendre en compte les
changements.

## Le contexte : une sélection courante

La quasi-totalité des commandes s'appuient sur un **contexte courant** persistant
(`PurposeOps-config/context.yaml`) qui mémorise le host, le customer et le deployment actuellement
sélectionnés — l'équivalent du "current context" de `kubectl`. On sélectionne un host (`sh`), puis un
customer (`sc`), puis un deployment (`sd`), et les commandes suivantes (backup, docker, etc.) opèrent
sur cette sélection sans qu'il soit nécessaire de la repréciser à chaque fois.

## Commandes principales

### Sélecteur interactif

| Commande | Description |
|---|---|
| `ppos [query]` | Sélecteur interactif (via `fzf` si disponible) listant toutes les commandes disponibles et les exécutant directement |

### Connexions SSH (`ssh-manager.nu`)

| Alias | Description |
|---|---|
| `close` | Fermer la connexion SSH master courante |
| `closeall` | Fermer toutes les connexions SSH master |
| `lsconn` | Lister les connexions SSH master actives |

### Contexte / prompt (`context/`)

| Alias | Description |
|---|---|
| `p` | Afficher le contexte courant (host/customer/deployment) dans le prompt |
| `t` | Activer/désactiver l'affichage du contexte dans le prompt |

### Hosts (`machine-manager/`, `config/`)

| Alias | Description |
|---|---|
| `h` | Détails de l'hôte courant |
| `lsh` | Lister tous les hosts configurés |
| `sh` | Sélectionner/changer de host courant |
| `ch` | Créer un nouveau host |
| `dh` | Supprimer un host |

### Customers (`customer-manager/`, `config/`)

| Alias | Description |
|---|---|
| `c` | Customer sélectionné dans le contexte |
| `sc` | Sélectionner/changer de customer courant |
| `lsc` | Lister tous les customers |
| `cc` | Créer un nouveau customer |
| `dc` | Supprimer un customer |

### Deployments (`deployment-manager/`)

| Alias | Description |
|---|---|
| `pde` | Id du deployment courant |
| `pdei` | Infos complètes du deployment courant |
| `sd` | Sélectionner/changer de deployment courant |
| `lsd` | Lister les deployments du customer courant |
| `cdep` | Créer un nouveau deployment pour le customer courant |

### Services (`config/`, `service-manager.nu`)

| Alias | Description |
|---|---|
| `lss` | Lister tous les services disponibles |
| `cs` | Créer un nouveau service |
| `ds` | Supprimer un service |

### Docker (`docker/`)

| Alias | Description |
|---|---|
| `dstart` | Démarrer les conteneurs Docker |
| `dstop` | Arrêter les conteneurs Docker |
| `drestart` | Redémarrer les conteneurs Docker |
| `dps` | Statut des conteneurs Docker |
| `dnls` | Lister les réseaux Docker |
| `dn extract` | Extraire les infos des réseaux Docker |

### Backup / Restore (`customer-manager/backup.nu`)

| Commande | Description |
|---|---|
| `backup run` | Lancer un backup pour le customer/deployment courant |
| `backup restore <backup_file>` | Restaurer un backup dans le deployment courant (destructif, `DROP DATABASE` — demande confirmation sauf `--force`) |

### Templates (`templater.nu`)

| Alias | Description |
|---|---|
| `g dc` | Générer un `docker-compose.yml` à partir d'un template (`templates/`) |

## Architecture du module

Chaque sous-système est un répertoire avec un `mod.nu` qui fait `export use core.nu *` (plus
`internal.nu`, `validations.nu` au besoin), et qui déclare les alias courts en bas de fichier. `ppo.nu`
à la racine ré-exporte tous les sous-systèmes et constitue le point d'entrée unique chargé par le shell.

Au sein d'un sous-système : `core.nu` porte les commandes publiques/interactives, `internal.nu` les
helpers internes d'écriture/mutation, `validations.nu` les vérifications de cohérence pures.

```
ppo.nu                        # point d'entrée, ré-exporte tous les sous-systèmes
config/                       # constantes de chemins + CRUD hosts/customers/services (YAML)
context/                      # contexte courant (host/customer/deployment sélectionnés) + prompt
customer-manager/             # gestion des customers + backup/restore
deployment-manager/           # gestion des deployments (rattachés à un customer)
docker/                       # exécution de commandes docker sur les hosts (local ou via SSH)
machine-manager/              # gestion des hosts
ssh-manager.nu                # connexions SSH ControlMaster (multiplexées, persistantes)
service-manager.nu            # listing des services disponibles
templater.nu                  # génération de docker-compose.yml à partir de templates
templates/                    # templates docker-compose (Caddy, Vaultwarden, ...)
PurposeOps-config/            # submodule git séparé : données réelles (hosts, customers, credentials)
```

Les données de configuration (hosts, customers, deployments, credentials DB, services) vivent dans le
submodule `PurposeOps-config` et non dans ce dépôt — toute création/modification d'entrée de config
nécessite un commit dans ce submodule également.

## Vérifier / tester

Il n'y a pas de suite de tests automatisée. Pour valider une modification :

```nu
# Vérification syntaxique d'un module
nu -c "nu-check <path/to/file.nu>"

# Charger le module à froid et exécuter une commande (sans dépendre d'un shell déjà chargé)
nu --no-config-file -c "use /home/ngner/dev/nu-modules/PurposeOps/ppo.nu; ppo <command>"
```

Sinon, exécuter la commande réelle contre un host/customer de test (ou réel) et inspecter la sortie et
les effets de bord sur l'hôte distant.

## Points d'attention

- Un `open | insert | save` sur un fichier YAML de config re-sérialise **tout le fichier** dans le style
  propre à Nushell (perte des commentaires, ré-indentation, changement de quoting) — sans perte de
  données, mais le diff peut toucher des parties non liées au changement voulu.
- Les parenthèses non échappées dans une interpolation de chaîne Nushell (`$"...(...)"`) déclenchent une
  substitution de commande, pas du texte littéral — toujours échapper `\(` `\)` pour du texte affiché,
  et `$\(...\)` si on veut injecter un `$(...)` bash littéral dans un script shell distant construit en
  chaîne Nushell.
- Un chemin `~/...` entouré de guillemets simples dans une commande distante n'est **pas** expansé par
  le shell distant (les guillemets simples suppriment aussi l'expansion du tilde) ; utiliser
  `resolve-remote-path` (`customer-manager/backup.nu`) avant d'injecter un tel chemin.
- `docker-compose-functions.nu` est à jour mais n'est pas encore ré-exporté depuis `ppo.nu` — il est
  donc actuellement inaccessible depuis la CLI `ppo`.
