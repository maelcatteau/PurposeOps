# customer-manager/backup.nu
# Module de backup Odoo contextualisé

use ../context/context-manager.nu *
use ../config/config-helper.nu *
use ../config/config.nu *
use ../docker/core.nu *
use ../deployment-manager *
use ../ssh-manager.nu run_with_master

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
    
    # 🛑 MODIFICATION ICI : On ne fait plus 'mkdir' localement !
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
# MOTEUR INTERNE
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
    
    let home_dir = "/home/ngner" 
    let clean_output_dir = ($outputDir | str replace "~" $home_dir)
    let remote_dest = $"($clean_output_dir)/($fname).tar.gz"

    # Exécute une commande docker distante et vérifie le résultat.
    # Affiche stderr et lève une erreur explicite en cas d'échec.
    def exec-remote-checked [args: list, step: string] {
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

    def exec-remote [args: list] {
        run_docker_command $args $host_info
    }

    # Exécute une commande shell brute (non-docker) sur l'hôte, via la connexion SSH master.
    def exec-remote-shell [cmd: string] {
        if ($host_info.hostname == "localhost") {
            run-external "sh" "-c" $cmd
        } else {
            run_with_master $host_info $cmd
        }
    }

    # Variante "checked" : capture aussi les erreurs levées avant que 'complete'
    # n'ait pu intercepter le résultat (ex: échec de spawn ssh), pour ne jamais
    # perdre le détail de l'erreur derrière un message générique.
    def exec-remote-shell-checked [cmd: string, step: string] {
        let result = (try {
            exec-remote-shell $cmd | complete
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

    try {
        # 0. S'assurer que le dossier de destination existe sur l'hôte distant
        print $"📁 Création du dossier distant si nécessaire \(($clean_output_dir)\)..."
        exec-remote-shell-checked $"mkdir -p '($clean_output_dir)'" "création du dossier distant"

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
        ] "dump SQL (pg_dump)"
        
        # 2. Rapatriement du SQL vers le conteneur APP via ton wrapper
        print "🔄 Centralisation du fichier SQL vers le conteneur applicatif..."
        exec-remote-checked ["cp", $"($db_container):($tmp)/($fname).sql", $"($tmp)/($fname).sql"] "copie SQL conteneur DB -> laptop"
        exec-remote-checked ["cp", $"($tmp)/($fname).sql", $"($app_container):($tmp)/($fname).sql"] "copie SQL laptop -> conteneur APP"
        
        exec-remote ["exec", $db_container, "rm", "-f", $"($tmp)/($fname).sql"] | complete
        exec-remote-shell $"rm -f '($tmp)/($fname).sql'" | complete

        # 3. Filestore -> Dans le conteneur APP
        print $"📂 Vérification du filestore \(depuis ($app_container)\)..."
        let fs_check_cmd = $"[ -d '/var/lib/odoo/filestore/($database)' ] && echo ok"
        let fs_check = (exec-remote ["exec", $app_container, "sh", "-c", $fs_check_cmd] | complete)
        
        if ($fs_check.stdout | str trim) == "ok" {
            print "📦 Compression du filestore..."
            let tar_fs_cmd = $"cd /var/lib/odoo/filestore && tar -czf '($tmp)/($fname)_fs.tar.gz' '($database)'"
            exec-remote-checked ["exec", $app_container, "sh", "-c", $tar_fs_cmd] "compression du filestore"
        } else {
            print "⚠️ Filestore absent dans l'application, création d'une archive vide..."
            exec-remote-checked ["exec", $app_container, "sh", "-c", $"mkdir -p ($tmp)/empty && tar -czf ($tmp)/($fname)_fs.tar.gz -C ($tmp)/empty ."] "archive filestore vide"
        }
        
        # 4. Archive finale -> Dans le conteneur APP
        print "📦 Création de l'archive globale..."
        let tar_all_cmd = $"cd '($tmp)' && tar -czf '($fname).tar.gz' '($fname).sql' '($fname)_fs.tar.gz'"
        exec-remote-checked ["exec", $app_container, "sh", "-c", $tar_all_cmd] "archive globale"
        
        # 5. Extraction finale vers l'hôte VPS
        print $"💾 Extraction vers le stockage du serveur [($remote_dest)]..."
        exec-remote-checked ["cp", $"($app_container):($tmp)/($fname).tar.gz", $"($clean_output_dir)/($fname).tar.gz"] "extraction finale vers l'hôte"
        
        # 6. Clean final des fichiers de travail dans l'APP
        # -u root : les fichiers copiés via 'docker cp' n'appartiennent pas forcément
        # à l'utilisateur par défaut du conteneur, qui ne peut alors pas les supprimer.
        print "🧹 Nettoyage des fichiers temporaires..."
        exec-remote ["exec", "-u", "root", $app_container, "rm", "-f", $"($tmp)/($fname).sql", $"($tmp)/($fname)_fs.tar.gz", $"($tmp)/($fname).tar.gz"] | complete

        print $"🎉 Succès ! Backup complet disponible sur le serveur : ($remote_dest)"

    } catch { |e|
        print $"❌ Erreur attrapée : ($e.msg)"
        print "⚠️ Tentative de nettoyage sécurisée..."
        exec-remote ["exec", $db_container, "rm", "-f", $"($tmp)/($fname).sql"] | complete
        exec-remote ["exec", "-u", "root", $app_container, "rm", "-f", $"($tmp)/($fname).sql", $"($tmp)/($fname)_fs.tar.gz", $"($tmp)/($fname).tar.gz"] | complete
        exec-remote-shell $"rm -f '($tmp)/($fname).sql'" | complete
        error make {msg: $e.msg}
    }
}