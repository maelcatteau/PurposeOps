# customer-manager/backup.nu
# Module de backup Odoo contextualisé

use ../context/context-manager.nu *
use ../config/config-helper.nu *
use ../config/config.nu *
use ../docker/core.nu *
use ../deployment-manager *
use ../ssh-manager.nu run_with_master

# ==============================================================================
# HELPERS PARTAGÉS
# ==============================================================================

# Remplace le '~' par le home distant en dur : un chemin '~/...' passé tel quel dans
# une commande entre quotes simples (nécessaire pour la construction des commandes shell
# distantes) n'est PAS étendu par le shell, puisque le tilde-expansion ne s'applique pas
# à l'intérieur de quotes simples. Et '| path expand' ne convient pas non plus : c'est le
# ~ du laptop local qu'il résoudrait, pas celui de l'utilisateur SSH sur l'hôte distant.
def resolve-remote-path [path: string] {
    let home_dir = "/home/ngner"
    $path | str replace "~" $home_dir
}

# Exécute une commande docker distante et vérifie le résultat.
# Affiche stderr et lève une erreur explicite en cas d'échec.
def exec-remote-checked [args: list, host_info: record, step: string] {
    let result = (run_docker_command $args $host_info | complete)
    if $result.exit_code != 0 {
        print $"❌ Échec à l'étape '($step)' \(code ($result.exit_code)\)"
        if not ($result.stderr | is-empty) {
            print $"   stderr : ($result.stderr)"
        }
        if not ($result.stdout | is-empty) {
            print $"   stdout : ($result.stdout)"
        }
        error make {msg: $"Échec de l'étape '($step)': ($result.stderr)"}
    }
    $result
}

def exec-remote [args: list, host_info: record] {
    run_docker_command $args $host_info
}

# Exécute une commande shell brute (non-docker) sur l'hôte, via la connexion SSH master.
def exec-remote-shell [cmd: string, host_info: record] {
    if ($host_info.hostname == "localhost") {
        run-external "sh" "-c" $cmd
    } else {
        run_with_master $host_info $cmd
    }
}

# Liste les backups (*.tar.gz) disponibles dans un dossier distant, du plus récent au plus ancien.
def list-remote-backups [dir: string, host_info: record] {
    let result = (exec-remote-shell $"ls -1t '($dir)' 2>/dev/null" $host_info | complete)
    $result.stdout | lines | where ($it | str ends-with ".tar.gz")
}

# Variante "checked" : capture aussi les erreurs levées avant que 'complete'
# n'ait pu intercepter le résultat (ex: échec de spawn ssh), pour ne jamais
# perdre le détail de l'erreur derrière un message générique.
def exec-remote-shell-checked [cmd: string, host_info: record, step: string] {
    let result = (try {
        exec-remote-shell $cmd $host_info | complete
    } catch { |e|
        {stdout: "", stderr: $e.msg, exit_code: -1}
    })
    if $result.exit_code != 0 {
        print $"❌ Échec à l'étape '($step)' \(code ($result.exit_code)\)"
        if not ($result.stderr | is-empty) {
            print $"   stderr : ($result.stderr)"
        }
        if not ($result.stdout | is-empty) {
            print $"   stdout : ($result.stdout)"
        }
        error make {msg: $"Échec de l'étape '($step)': ($result.stderr)"}
    }
    $result
}

# ==============================================================================
# COMMANDE PRINCIPALE : backup run
# ==============================================================================
export def "backup run" [
    --service: string = "",
    --cron,
    --silent,
    --output-dir: string = ""
] {
    # 1. Charger le contexte enrichi
    let ctx = (load_context)

    # Vérifications de base
    if ($ctx | get customer | columns | is-empty) {
        error make {msg: "❌ Aucun client sélectionné."}
    }
    let customerName = ($ctx | get customer | columns | first)
    let customerData = ($ctx | get customer | get $customerName)

    # 2. Récupérer le déploiement COMPLET directement depuis le contexte
    let deployment = (get-current-deployment-info)

    let targetServiceName = ($deployment.service_name)
    let hostId = ($deployment.hosts | get 0.host_id)
    let baseDeployPath = ($deployment.hosts | get 0.path_for_service)
    let db_name = ($deployment | get -o database_name)

     # 3. Infos Hôte & Docker Context
    let hostsConfig = (open $hosts_config_path)
    let hostInfo = ($hostsConfig | get $hostId)

    # 4. Trouver le conteneur
    let appContainerName = $deployment | get container_name
    let dbContainerName = $deployment | get db_container_name

    print $"📦 Conteneur identifié : ($appContainerName)"

    # 5. Credentials DB
    let db_creds = ($deployment | get -o db_credentials)
    if ($db_creds | is-empty) {
        error make {msg: "❌ Credentials DB manquants. Ajoutez 'db_credentials' dans customers.yaml."}
    }

    let dbHost = ($db_creds | get host)
    let dbPort = ($db_creds | get port)
    let dbUser = ($db_creds | get user)
    let dbPassword = ($db_creds | get password)

    print $"✅ Credentials chargés : User=($dbUser), Host=($dbHost)"

    # 6. Chemin de sortie (SUR LE SERVEUR)
    # Note : On n'utilise plus '| path expand' ici car le chemin local (~ de ton laptop)
    # n'est pas forcément le même que celui du serveur (~ du user SSH).
    let finalOutputDir = if ($output_dir | is-empty) {
        let abbrev = ($customerData | get abbreviation)
        if ($abbrev | is-empty) {
            error make {msg: "Abréviation client manquante."}
        }
        # On utilise une String brute qui sera interprétée par le serveur distant
        $"~/backups/($abbrev)/($hostId)"
    } else {
        $output_dir
    }

    print $"📁 Dossier de backup cible sur le serveur : ($finalOutputDir)"

    # === SÉCURITÉ / DEBUG ===
    print "🔍 DEBUG VARIABLES :"
    print $"   - customerName: ($customerName)"
    print $"   - db_name: ($db_name)"
    print $"   - appContainerName: ($appContainerName)"
    print $"   - dbContainerName: ($dbContainerName)"
    print $"   - hostId: ($hostId)"

    # 7. Exécution
    print "🚀 Backup en cours..."
    do-generic-backup $customerName $db_name $appContainerName $dbContainerName $hostInfo --dbHost $dbHost --dbPort $dbPort --dbUser $dbUser --dbPassword $dbPassword --outputDir $finalOutputDir --cron=$cron --silent=$silent
}

# ==============================================================================
# COMMANDE PRINCIPALE : backup restore
# ==============================================================================
export def "backup restore" [
    backup_file?: string,              # Nom de fichier (résolu dans le dossier de backup du client courant) ou chemin absolu sur l'hôte cible ; si omis, sélection interactive parmi les backups du déploiement courant
    --target-database: string = "",    # Base de destination (par défaut : database_name du déploiement courant)
    --force,                           # Ne pas demander de confirmation avant d'écraser la base cible
    --silent
] {
    # 1. Charger le contexte enrichi
    let ctx = (load_context)

    if ($ctx | get customer | columns | is-empty) {
        error make {msg: "❌ Aucun client sélectionné."}
    }
    let customerName = ($ctx | get customer | columns | first)
    let customerData = ($ctx | get customer | get $customerName)

    # 2. Récupérer le déploiement COMPLET directement depuis le contexte
    let deployment = (get-current-deployment-info)
    let hostId = ($deployment.hosts | get 0.host_id)

    let hostsConfig = (open $hosts_config_path)
    let hostInfo = ($hostsConfig | get $hostId)

    let appContainerName = $deployment | get container_name
    let dbContainerName = $deployment | get db_container_name

    print $"📦 Conteneur identifié : ($appContainerName)"

    # 3. Credentials DB
    let db_creds = ($deployment | get -o db_credentials)
    if ($db_creds | is-empty) {
        error make {msg: "❌ Credentials DB manquants. Ajoutez 'db_credentials' dans customers.yaml."}
    }

    let dbHost = ($db_creds | get host)
    let dbPort = ($db_creds | get port)
    let dbUser = ($db_creds | get user)
    let dbPassword = ($db_creds | get password)

    # 4. Base de destination
    let targetDatabase = if ($target_database | is-empty) {
        ($deployment | get -o database_name)
    } else {
        $target_database
    }

    if ($targetDatabase | is-empty) {
        error make {msg: "❌ Aucune base de données cible. Précisez --target-database ou configurez database_name pour ce déploiement."}
    }

    let abbrev = ($customerData | get -o abbreviation)

    # 5. Sélection du backup si aucun fichier n'a été donné explicitement
    let backup_file = if ($backup_file | is-empty) {
        if ($abbrev | is-empty) {
            error make {msg: "Abréviation client manquante, impossible de lister les backups."}
        }
        let backup_dir = (resolve-remote-path $"~/backups/($abbrev)/($hostId)")
        print $"🔎 Recherche des backups disponibles \(($backup_dir)\)..."
        let available = (list-remote-backups $backup_dir $hostInfo)

        if ($available | is-empty) {
            error make {msg: $"❌ Aucun backup trouvé dans ($backup_dir)."}
        }

        let selected = if (which fzf | is-not-empty) {
            $available | to text | fzf --prompt="Backup à restaurer> "
        } else {
            $available | input list "Sélectionnez un backup à restaurer : "
        }

        if ($selected | is-empty) {
            print "❌ Restauration annulée."
            return
        }
        $selected
    } else {
        $backup_file
    }

    # 6. Résolution du chemin du backup SUR L'HÔTE CIBLE
    # Un backup peut venir d'un tout autre client/déploiement (restauration croisée) :
    # un chemin absolu est utilisé tel quel, sinon on le cherche dans le dossier de
    # backup habituel du client courant.
    let backupPath = if ($backup_file | str starts-with "/") or ($backup_file | str starts-with "~") {
        resolve-remote-path $backup_file
    } else {
        if ($abbrev | is-empty) {
            error make {msg: "Abréviation client manquante et chemin de backup non-absolu fourni."}
        }
        resolve-remote-path $"~/backups/($abbrev)/($hostId)/($backup_file)"
    }

    print "🔄 RESTAURATION ODOO"
    print $"📋 Client          : ($customerName)"
    print $"📋 Base cible      : ($targetDatabase)"
    print $"📋 Backup          : ($backupPath)"

    # 7. Confirmation (opération destructive : DROP DATABASE sur la cible)
    if not $force {
        print $"⚠️ Ceci va DÉTRUIRE le contenu actuel de la base '($targetDatabase)' sur ($appContainerName)."
        let validation = (input "Continuer ? [y/n] ")
        if $validation != "y" {
            print "❌ Restauration annulée."
            return
        }
    }

    do-generic-restore $customerName $targetDatabase $appContainerName $dbContainerName $hostInfo $backupPath --dbHost $dbHost --dbPort $dbPort --dbUser $dbUser --dbPassword $dbPassword --silent=$silent
}

# ==============================================================================
# MOTEUR INTERNE : backup
# ==============================================================================
def do-generic-backup [
    customer: string, database: string, app_container: string, db_container: string, host_info: record,
    --dbHost: string, --dbPort: string, --dbUser: string, --dbPassword: string,
    --outputDir: string, --cron, --silent
] {
    let ts = (date now | format date "%Y%m%d_%H%M%S")
    let prefix = if $cron { "cron" } else { "manual" }
    let fname = $"($prefix)_($database)_($ts)"
    let tmp = "/tmp"

    let clean_output_dir = (resolve-remote-path $outputDir)
    let remote_dest = $"($clean_output_dir)/($fname).tar.gz"

    try {
        # 0. S'assurer que le dossier de destination existe sur l'hôte distant
        print $"📁 Création du dossier distant si nécessaire \(($clean_output_dir)\)..."
        exec-remote-shell-checked $"mkdir -p '($clean_output_dir)'" $host_info "création du dossier distant"

        # 1. Dump SQL depuis le conteneur DB vers le /tmp du conteneur DB
        print $"🗄️ Dump de la base de données \(depuis ($db_container)\)..."
        exec-remote-checked [
            "exec",
            "-e", $"PGPASSWORD=($dbPassword)",
            $db_container,
            "pg_dump",
            "-h", "localhost",
            "-p", $dbPort,
            "-U", $dbUser,
            "-d", $database,
            "-f", $"($tmp)/($fname).sql"
        ] $host_info "dump SQL (pg_dump)"

        # 2. Rapatriement du SQL vers le conteneur APP via ton wrapper
        print "🔄 Centralisation du fichier SQL vers le conteneur applicatif..."
        exec-remote-checked ["cp", $"($db_container):($tmp)/($fname).sql", $"($tmp)/($fname).sql"] $host_info "copie SQL conteneur DB -> laptop"
        exec-remote-checked ["cp", $"($tmp)/($fname).sql", $"($app_container):($tmp)/($fname).sql"] $host_info "copie SQL laptop -> conteneur APP"

        exec-remote ["exec", $db_container, "rm", "-f", $"($tmp)/($fname).sql"] $host_info | complete
        exec-remote-shell $"rm -f '($tmp)/($fname).sql'" $host_info | complete

        # 3. Filestore -> Dans le conteneur APP
        print $"📂 Vérification du filestore \(depuis ($app_container)\)..."
        let fs_check_cmd = $"[ -d '/var/lib/odoo/filestore/($database)' ] && echo ok"
        let fs_check = (exec-remote ["exec", $app_container, "sh", "-c", $fs_check_cmd] $host_info | complete)

        if ($fs_check.stdout | str trim) == "ok" {
            print "📦 Compression du filestore..."
            let tar_fs_cmd = $"cd /var/lib/odoo/filestore && tar -czf '($tmp)/($fname)_fs.tar.gz' '($database)'"
            exec-remote-checked ["exec", $app_container, "sh", "-c", $tar_fs_cmd] $host_info "compression du filestore"
        } else {
            print "⚠️ Filestore absent dans l'application, création d'une archive vide..."
            exec-remote-checked ["exec", $app_container, "sh", "-c", $"mkdir -p ($tmp)/empty && tar -czf ($tmp)/($fname)_fs.tar.gz -C ($tmp)/empty ."] $host_info "archive filestore vide"
        }

        # 4. Archive finale -> Dans le conteneur APP
        print "📦 Création de l'archive globale..."
        let tar_all_cmd = $"cd '($tmp)' && tar -czf '($fname).tar.gz' '($fname).sql' '($fname)_fs.tar.gz'"
        exec-remote-checked ["exec", $app_container, "sh", "-c", $tar_all_cmd] $host_info "archive globale"

        # 5. Extraction finale vers l'hôte VPS
        print $"💾 Extraction vers le stockage du serveur [($remote_dest)]..."
        exec-remote-checked ["cp", $"($app_container):($tmp)/($fname).tar.gz", $"($clean_output_dir)/($fname).tar.gz"] $host_info "extraction finale vers l'hôte"

        # 6. Clean final des fichiers de travail dans l'APP
        # -u root : les fichiers copiés via 'docker cp' n'appartiennent pas forcément
        # à l'utilisateur par défaut du conteneur, qui ne peut alors pas les supprimer.
        print "🧹 Nettoyage des fichiers temporaires..."
        exec-remote ["exec", "-u", "root", $app_container, "rm", "-f", $"($tmp)/($fname).sql", $"($tmp)/($fname)_fs.tar.gz", $"($tmp)/($fname).tar.gz"] $host_info | complete

        print $"🎉 Succès ! Backup complet disponible sur le serveur : ($remote_dest)"

    } catch { |e|
        print $"❌ Erreur attrapée : ($e.msg)"
        print "⚠️ Tentative de nettoyage sécurisée..."
        exec-remote ["exec", $db_container, "rm", "-f", $"($tmp)/($fname).sql"] $host_info | complete
        exec-remote ["exec", "-u", "root", $app_container, "rm", "-f", $"($tmp)/($fname).sql", $"($tmp)/($fname)_fs.tar.gz", $"($tmp)/($fname).tar.gz"] $host_info | complete
        exec-remote-shell $"rm -f '($tmp)/($fname).sql'" $host_info | complete
        error make {msg: $e.msg}
    }
}

# ==============================================================================
# MOTEUR INTERNE : restore
# ==============================================================================
def do-generic-restore [
    customer: string, target_database: string, app_container: string, db_container: string, host_info: record, backup_path: string,
    --dbHost: string, --dbPort: string, --dbUser: string, --dbPassword: string,
    --silent
] {
    let tmp = "/tmp"
    let ts = (date now | format date "%Y%m%d_%H%M%S")
    let work_dir = $"($tmp)/restore_($ts)"

    try {
        # 0. Vérifier que le fichier de backup existe bien sur l'hôte
        print "🔎 Vérification du fichier de backup sur l'hôte..."
        exec-remote-shell-checked $"test -f '($backup_path)'" $host_info "vérification du fichier de backup"

        # 1. Arrêt de l'application : des connexions actives empêcheraient le DROP DATABASE
        print $"🛑 Arrêt du conteneur applicatif \(($app_container)\)..."
        exec-remote-checked ["stop", $app_container] $host_info "arrêt du conteneur applicatif"

        # 2. Extraction de l'archive sur l'hôte (le .sql et le _fs.tar.gz qu'elle contient
        # portent le nom de la base D'ORIGINE, pas forcément celui de la cible)
        print "📦 Extraction de l'archive sur l'hôte..."
        exec-remote-shell-checked $"mkdir -p '($work_dir)' && tar -xzf '($backup_path)' -C '($work_dir)'" $host_info "extraction de l'archive"

        let sql_find = (exec-remote-shell $"ls ($work_dir)/*.sql 2>/dev/null | head -1" $host_info | complete)
        let sql_file = ($sql_find.stdout | str trim)
        if ($sql_file | is-empty) {
            error make {msg: "Aucun fichier .sql trouvé dans l'archive de backup."}
        }

        let fs_find = (exec-remote-shell $"ls ($work_dir)/*_fs.tar.gz 2>/dev/null | head -1" $host_info | complete)
        let fs_archive = ($fs_find.stdout | str trim)

        # 3. DROP + CREATE de la base cible
        print $"🗑️ Suppression de la base '($target_database)' si elle existe..."
        exec-remote-checked ["exec", "-e", $"PGPASSWORD=($dbPassword)", $db_container, "psql", "-h", "localhost", "-p", $dbPort, "-U", $dbUser, "-d", "postgres", "-c", $"DROP DATABASE IF EXISTS \"($target_database)\""] $host_info "suppression de la base existante"

        print $"🆕 Création de la base '($target_database)'..."
        exec-remote-checked ["exec", "-e", $"PGPASSWORD=($dbPassword)", $db_container, "psql", "-h", "localhost", "-p", $dbPort, "-U", $dbUser, "-d", "postgres", "-c", $"CREATE DATABASE \"($target_database)\" OWNER \"($dbUser)\" ENCODING 'UTF8'"] $host_info "création de la base cible"

        # 4. Copier le dump SQL dans le conteneur DB et le restaurer
        print "💾 Copie et restauration du dump SQL..."
        exec-remote-checked ["cp", $sql_file, $"($db_container):($tmp)/restore_($ts).sql"] $host_info "copie du dump vers le conteneur DB"
        exec-remote-checked ["exec", "-e", $"PGPASSWORD=($dbPassword)", $db_container, "psql", "-h", "localhost", "-p", $dbPort, "-U", $dbUser, "-d", $target_database, "-f", $"($tmp)/restore_($ts).sql"] $host_info "restauration du dump SQL"
        exec-remote ["exec", $db_container, "rm", "-f", $"($tmp)/restore_($ts).sql"] $host_info | complete

        # 5. Restaurer le filestore si présent (une archive vide -> pas de filestore à restaurer)
        # NOTE : le conteneur applicatif est arrêté à ce stade (nécessaire pour le DROP DATABASE
        # plus haut), donc 'docker exec' est indisponible - tout se fait via des commandes shell
        # sur l'HÔTE et 'docker cp' (qui fonctionne sur un conteneur arrêté, contrairement à exec).
        let has_filestore = not ($fs_archive | is-empty)
        if $has_filestore {
            print "📂 Restauration du filestore..."
            let fs_extract_dir = $"($work_dir)/fs_extract"
            exec-remote-shell-checked $"mkdir -p '($fs_extract_dir)' && tar -xzf '($fs_archive)' -C '($fs_extract_dir)'" $host_info "extraction du filestore"

            # L'archive contient un répertoire top-level nommé d'après la base D'ORIGINE ;
            # on en recopie le contenu (pas le répertoire lui-même) vers le filestore de la cible.
            let src_dir_result = (exec-remote-shell $"ls '($fs_extract_dir)'" $host_info | complete)
            let src_dir_name = ($src_dir_result.stdout | str trim)

            if ($src_dir_name | is-empty) {
                print "⚠️ Archive de filestore vide, rien à restaurer."
            } else {
                # Le suffixe '/.' copie le CONTENU du répertoire source, pas le répertoire lui-même.
                exec-remote-checked ["cp", $"($fs_extract_dir)/($src_dir_name)/.", $"($app_container):/var/lib/odoo/filestore/($target_database)"] $host_info "copie du filestore vers le conteneur APP"
            }
        } else {
            print "⚠️ Aucun filestore dans l'archive, restauration SQL uniquement."
        }

        # 6. Nettoyage de l'hôte
        print "🧹 Nettoyage des fichiers temporaires sur l'hôte..."
        exec-remote-shell $"rm -rf '($work_dir)'" $host_info | complete

        # 7. Redémarrage de l'application (nécessaire avant tout 'docker exec' dessus)
        print $"🚀 Redémarrage du conteneur applicatif \(($app_container)\)..."
        exec-remote-checked ["start", $app_container] $host_info "redémarrage du conteneur applicatif"

        # 8. Les fichiers copiés via 'docker cp' n'appartiennent pas forcément à l'utilisateur
        # par défaut du conteneur - on corrige une fois celui-ci de nouveau démarré.
        if $has_filestore {
            exec-remote ["exec", "-u", "root", $app_container, "chown", "-R", "odoo:odoo", $"/var/lib/odoo/filestore/($target_database)"] $host_info | complete
        }

        print $"🎉 Succès ! Base '($target_database)' restaurée depuis ($backup_path)"

    } catch { |e|
        print $"❌ Erreur attrapée : ($e.msg)"
        print "⚠️ Tentative de redémarrage du conteneur applicatif..."
        exec-remote ["start", $app_container] $host_info | complete
        exec-remote-shell $"rm -rf '($work_dir)'" $host_info | complete
        error make {msg: $e.msg}
    }
}
