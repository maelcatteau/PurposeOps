###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use context-manager.nu *
use ssh-manager.nu *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Fonctions helper internes                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Fonction helper pour sélectionner les bonnes informations pour le bon type d'opérations
def get_config [operation: string] {
    match $operation {
        "start" => {
            need_all: true
            header: "Select a container to start :"
            verb: "Starting"
            past_participle: "started"
        }
        "stop" => {
            need_all: false
            header: "Select a container to stop :"
            verb: "Stopping"
            past_participle: "stopped"
        }
        "restart" => {
            need_all: false
            header: "Select a container to restart :"
            verb: "Restarting"
            past_participle: "restarted"
        }
        "networks_extract" => {
            need_all: true
            header: "Select a container to extract networks from :"
            verb: "Extracting networks from"
            past_participle: "networks extracted from"
        }
        _ => {
            error make {msg: $"Configuration non trouvée pour l'opération: ($operation)"}
        }
    }
}

# Fonction helper pour récupérer le container depuis la sortie du fuzzy finder 
def get_container_name_from_fzf [fzf_output: string] {
    let container_name = ($fzf_output | str replace -a "│" "" 
        | str trim 
        | split row " " 
        | where $it != "" 
        | skip 1 
        | first)
    return $container_name
}

# Nouvelle fonction qui évite les problèmes d'échappement
def get_containers_list [
    format_string: string,
    --all(-a)  # Flag pour inclure tous les containers (arrêtés inclus)
] {
    # Récupérer les données en JSON (pas de problème d'échappement)
    let raw_data = if $all {
        run_docker_command ["ps" "-a" "--format" "json"]
    } else {
        run_docker_command ["ps" "--format" "json"]
    }
    
    # Parser et formater côté Nushell
    $raw_data 
    | lines 
    | where $it != ""
    | each { |line| $line | from json }
    | each { |container|
        # Extraire les champs qu'on veut (équivalent à votre format_string)
        $"($container.Names)\t($container.Image)\t($container.Status)"
    }
    | str join "\n"
}

# Fonction pour vérifier si le contenu est vide (retourne juste un boolean)
def is_empty_content [content: string] {
    ($content | str trim | is-empty)
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Fonctions publiques                                               #######################
###########################################################################################################################################################
###########################################################################################################################################################
export def run_docker_command [command: list] {
    let host_info = get-current-host-info
    let current_host = get-current-host

    if $current_host == "localhost" or $host_info.hostname == "localhost" {
        run-external "docker" ...$command
    } else {
        let docker_cmd_string = (["docker"] | append $command | str join " ")
        
        run_with_master $host_info $docker_cmd_string
    }
}



# Fonction générique pour les opérations Docker
export def docker_container_operation [
    --start(-s),      # Flag pour démarrer
    --stop(-p),       # Flag pour arrêter  
    --restart(-r),    # Flag pour redémarrer
    --networks(-n)    # Flag pour extraire les réseaux
] {
    # Déterminer l'opération en fonction des flags
    let operation = if $start {
        "start"
    } else if $stop {
        "stop"
    } else if $restart {
        "restart"
    } else if $networks {
        "networks_extract"
    } else {
        print "❌ Vous devez spécifier une opération : --start, --stop, --restart, ou --networks"
        return
    }

    # Configuration pour chaque opération
    let config = get_config $operation

    # Récupération des containers
    let containers_list = if $config.need_all {
        get_containers_list "{{.Names}}\t{{.Image}}\t{{.Status}}" --all
    } else {
        get_containers_list "{{.Names}}\t{{.Image}}\t{{.Status}}"
    }

    # Vérification si des containers sont disponibles
    if (is_empty_content $containers_list) {
        print $"Aucun container disponible pour ($operation)"
        return
    }

    # Sélection avec fzf
    let selected = try {
        $containers_list | lines | fzf --header=$config.header
    } catch {
        ""  # Si fzf est annulé, retourne une string vide
    }

    # Vérification de la sélection
    if (is_empty_content $selected) {
        print "Opération annulée - aucun container sélectionné"
        return
    }

    # Extraction du nom du container
    let container_name = get_container_name_from_fzf $selected

    # Exécution de l'opération selon le type
    if $operation == "networks_extract" {
        print $"($config.verb) container: ($container_name)"
        let networks = run_docker_command ["inspect" $container_name] | from json | get NetworkSettings.Networks
        
        if $env.LAST_EXIT_CODE == 0 {
            print $"✅ ($config.past_participle) container ($container_name)"
            return $networks
        } else {
            print $"❌ Failed to extract networks from container ($container_name)"
            return
        }
    } else {
        # Opérations standard (start, stop, restart)
        print $"($config.verb) container: ($container_name)"
        run_docker_command [$operation $container_name]

        # Vérification du résultat
        if $env.LAST_EXIT_CODE == 0 {
            print $"✅ Container ($container_name) ($config.past_participle) successfully"
        } else {
            print $"❌ Failed to ($operation) container ($container_name)"
        }
    }
}

# Show status of all containers
export def status [
    filter?: string     # Optional filter on container names
    --ports(-p)        # Flag to display ports
] {
    let base_data = if ($filter == null) {
        run_docker_command ["ps"] | from ssv -a
    } else {
        run_docker_command ["ps"] | from ssv -a | where NAMES =~ $filter
    }

    if $ports {
        $base_data | select NAMES IMAGE STATUS PORTS
    } else {
        $base_data | select NAMES IMAGE STATUS
    }
}

# Fonction pour lister la liste des réseaux existant
export def network_list [
    filter?: string    # Optional filter on network names
] {
    let base_data = if ($filter == null) {
        run_docker_command ["network", "ls"] | from ssv -a
    } else {
        run_docker_command ["network", "ls"] | from ssv -a | where NAME =~ $filter
    }
    $base_data | select NAME DRIVER SCOPE
}



###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo dstop" = docker_container_operation --stop
export alias "ppo dstart" = docker_container_operation --start
export alias "ppo drestart" = docker_container_operation --restart
export alias "ppo dnetextract" = docker_container_operation --networks
export alias "ppo dps" = status
export alias "ppo dnls" = network_list
