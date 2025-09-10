###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Fonctions helper internes                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################
def get_context_file_path [] {
    "~/dev/nu-modules/PurposeOps/context.json" | path expand
}

export def load_context [] {
    let context_path = get_context_file_path
    if not ($context_path | path exists) {
        # Créer le fichier de contexte par défaut s'il n'existe pas
        create_default_context
    }
    open $context_path
}

# Sauvegarder le contexte
def save_context [context: record] {
    let context_path = get_context_file_path
    $context | to json | save -f $context_path
}

def prepare_hosts_for_fzf [config: record, current_host: string] {
    $config.hosts 
    | transpose host info 
    | each {|row|
        let status = if ($row.host == $current_host) { " 👉 CURRENT" } else { "" }
        let type_icon = if ($row.info.hostname == "localhost") { "🏠" } else { "🌐" }
        
        # Format similaire à vos containers : ICON │ HOST_NAME │ DESCRIPTION │ STATUS
        $"($type_icon) │ ($row.host) │ ($row.info.name)($status)"
    }
}

def extract_host_from_fzf [selected_line: string] {
    print $"🔍 Extraction depuis: '($selected_line)'"
    
    # Diviser par │ et nettoyer chaque partie
    let parts = ($selected_line 
        | split row "│" 
        | each { |part| $part | str trim }
        | where $it != "")
    
    print $"📝 Parties nettoyées: ($parts)"
    
    # Structure attendue: [index, icône, nom_hôte, description]
    # Le nom d'hôte est à l'index 2 (3ème élément)
    if ($parts | length) >= 3 {
        let host_name = ($parts | get 2)
        print $"✅ Hôte extrait: '($host_name)'"
        return $host_name
    }
    
    print $"❌ Format inattendu - pas assez de parties ($parts | length)"
    return ""
}


# Logique interne pour changer d'hôte (factorisation)
def set_host_internal [host: string, config: record] {
    let host_info = ($config.hosts | get $host)

    # Créer le nouveau contexte avec l'hôte sélectionné
    let new_context = {
        host: {
            $host: $host_info
        }
    }

    # Sauvegarder le contexte
    save_context $new_context
    print $"📍 Context set to: ($host_info.name)"
}


###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Fonctions publiques                                               #######################
###########################################################################################################################################################
###########################################################################################################################################################

export def create_default_context [] {
    let context_path = get_context_file_path
    let config = load_config
    let localhost_info = ($config.hosts | get localhost)
    
    let default_context = {
        host: {
            localhost: $localhost_info
        }
    }
    
    # Créer le dossier s'il n'existe pas
    mkdir ($context_path | path dirname)
    $default_context | to json | save -f $context_path
}

export def load_config [] {
    let config_path = "./PurposeOps/config.json"
    if not ($config_path | path exists) {
        error make {msg: "Configuration file not found"}
    }
    open $config_path
}

export def resolve_key_path [identity_file: string] {
    if ($identity_file | str starts-with "~/") {
        $identity_file | str replace "~" $env.HOME
    } else if ($identity_file | str starts-with "./") {
        $identity_file | path expand
    } else {
        $identity_file
    }
}

# Fonction pour changer d'hôte (avec fuzzy finder)
export def set-host [host?: string] {  # <- Paramètre optionnel maintenant
    let config = load_config
    let current_host = get-current-host

    # Si un hôte est spécifié directement, utiliser l'ancienne logique
    if $host != null {
        if not ($host in $config.hosts) {
            print $"❌ Host '($host)' not found in configuration"
            print $"Available hosts: ($config.hosts | columns | str join ', ')"
            return
        }
        
        set_host_internal $host $config
        return
    }

    # Sinon, utiliser fzf pour la sélection interactive
    let hosts_info = prepare_hosts_for_fzf $config $current_host
    
    # Vérifier qu'on a des hôtes
    if ($hosts_info | is-empty) {
        print "❌ Aucun hôte disponible dans la configuration"
        return
    }

    # Sélection avec fzf
    let selected = try {
        $hosts_info | fzf --header="🖥️  Sélectionnez un hôte" --height=40%
    } catch {
        ""  # Si fzf est annulé
    }

    # Vérifier la sélection
    if ($selected | str trim | is-empty) {
        print "Opération annulée - aucun hôte sélectionné"
        return
    }

    # Extraire le nom de l'hôte sélectionné (première colonne)
    let selected_host = extract_host_from_fzf $selected
    
    # Changer vers l'hôte sélectionné
    set_host_internal $selected_host $config
}


# Obtenir l'hôte actuel
export def get-current-host [] {
    let context = load_context
    $context.host | columns | first
}

# Obtenir les informations de l'hôte actuel
export def get-current-host-info [] {
    let context = load_context
    let host_name = ($context.host | columns | first)
    $context.host | get $host_name
}

# Fonction pour lister les hôtes disponibles
export def list-hosts [] {
    let config = load_config
    let current_host = get-current-host
    
    $config.hosts | transpose host info | each {|row|
        {
            host: $row.host
            name: $row.info.name
            type: (if ($row.info.hostname == "localhost") { "local" } else { "remote" })
            current: ($row.host == $current_host)
        }
    }
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo host" = get-current-host-info
export alias "ppo hostname" = get-current-host
export alias "ppo lshost" = list-hosts
export alias "ppo shost" = set-host