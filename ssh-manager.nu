use context-manager.nu *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                              SSH Control Master                                              #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Fonction pour obtenir le chemin du dossier des sockets de contrôle
def get_control_path [] {
    let control_dir = "~/dev/nu-modules/PurposeOps/controlmasters" | path expand
    
    # Créer le dossier s'il n'existe pas
    if not ($control_dir | path exists) {
        mkdir $control_dir
    }
    
    $control_dir
}

# Fonction pour générer le nom du socket de contrôle
def get_control_socket [host_info: record] {
    let control_dir = get_control_path
    let socket_name = $"($host_info.user)@($host_info.hostname):($host_info.port)"
    $"($control_dir)/($socket_name)"
}

# Fonction pour vérifier si une connexion maître existe et est active
export def is_master_active [host_info: record] {
    let socket_path = get_control_socket $host_info
    
    # Vérifier si le socket existe
    if not ($socket_path | path exists) {
        return false
    }
    
    # Tester si la connexion est vraiment active
    let ssh_target = $"($host_info.user)@($host_info.hostname)"
    let test_result = try {
        run-external "ssh" "-O" "check" "-S" $socket_path $ssh_target
        true
    } catch {
        false
    }
    
    $test_result
}

# Fonction pour créer une connexion maître
export def create_master_connection [host_info: record] {
    let socket_path = get_control_socket $host_info
    let ssh_target = $"($host_info.user)@($host_info.hostname)"
    
    print $"🔄 Creating master connection to ($ssh_target)..."
    
    # Construire les arguments SSH
    let ssh_args = [
        "-M"                          # Mode Master
        "-N"                          # Pas de commande (juste la connexion)
        "-f"                          # En arrière-plan
        "-S" $socket_path             # Chemin du socket
        "-p" $host_info.port          # Port
    ]
    
    # Ajouter la clé privée si spécifiée
    let ssh_args = if ($host_info.identity_file != "") {
        $ssh_args | append ["-i" $host_info.identity_file]
    } else {
        $ssh_args
    }
    
    # Créer la connexion maître
    let result = try {
        run-external "ssh" ...$ssh_args $ssh_target
        true
    } catch { |err|
        print $"❌ Failed to create master connection: ($err.msg)"
        false
    }
    
    $result
}

# Fonction pour exécuter une commande via la connexion maître
export def run_with_master [host_info: record, command: string] {
    let socket_path = get_control_socket $host_info
    let ssh_target = $"($host_info.user)@($host_info.hostname)"
    
    # S'assurer qu'on a une connexion maître active
    if not (is_master_active $host_info) {
        if not (create_master_connection $host_info) {
            error make {msg: "Failed to establish master connection"}
        }
    }
    
    # CORRECTION: Échapper les accolades pour SSH
    let escaped_command = $command | str replace --all "{{" "\\{\\{" | str replace --all "}}" "\\}\\}"
    
    # Construire les arguments SSH
    let ssh_args = [
        "-S" $socket_path
        "-p" $host_info.port
        "-o" "StrictHostKeyChecking=no"
        "-o" "ConnectTimeout=10"
    ]
    
    # Ajouter la clé privée si nécessaire
    let ssh_args = if ($host_info.identity_file != "") {
        let key_path = resolve_key_path $host_info.identity_file
        $ssh_args | append ["-i" $key_path]
    } else {
        $ssh_args
    }
    
    # Exécuter la commande via la connexion maître
    run-external "ssh" ...$ssh_args $ssh_target $escaped_command
}

# Fonction pour fermer une connexion maître spécifique
export def close_master_connection [host_info: record] {
    let socket_path = get_control_socket $host_info
    let ssh_target = $"($host_info.user)@($host_info.hostname)"

    # Vérifier si une connexion existe
    if not ($socket_path | path exists) {
        print $"ℹ️  No master connection exists for ($ssh_target)"
        return true
    }

    # Vérifier si la connexion est active
    if not (is_master_active $host_info) {
        print $"ℹ️  Master connection for ($ssh_target) is already inactive"
        # Nettoyer le socket orphelin
        rm $socket_path
        return true
    }

    print $"🔄 Closing master connection to ($ssh_target)..."

    # Fermer la connexion maître
    let result = try {
        run-external "ssh" "-O" "exit" "-S" $socket_path $ssh_target
        print $"✅ Master connection closed for ($ssh_target)"
        true
    } catch { |err|
        print $"❌ Failed to close master connection: ($err.msg)"
        false
    }

    # Nettoyer le socket s'il existe encore
    if ($socket_path | path exists) {
        try {
            rm $socket_path
        } catch {
            print $"⚠️  Warning: Could not remove socket file ($socket_path)"
        }
    }

    $result
}

# Fonction pour fermer toutes les connexions maîtres actives
export def close_all_master_connections [] {
    let control_dir = get_control_path
    
    print "🔄 Closing all master connections..."
    
    # Chercher tous les sockets dans le répertoire de contrôle
    let sockets = try {
        ls $control_dir | where type == file | get name
    } catch {
        []
    }
    
    if ($sockets | is-empty) {
        print "ℹ️  No master connections found"
        return
    }
    
    mut closed_count = 0
    
    # Fermer chaque connexion
    for socket_path in $sockets {
        let socket_name = ($socket_path | path basename)
        print $"🔄 Processing ($socket_name)..."
        
        # Extraire les infos de connexion depuis le nom du socket
        # Format: user@hostname:port
        let parts = ($socket_name | parse "{user}@{hostname}:{port}")
        
        if ($parts | length) > 0 {
            let conn_info = $parts | first
            let host_info = {
                user: $conn_info.user
                hostname: $conn_info.hostname  
                port: ($conn_info.port | into int)
                identity_file: ""  # On n'a pas cette info depuis le socket
            }
            
            try {
                let ssh_target = $"($host_info.user)@($host_info.hostname)"
                run-external "ssh" "-O" "exit" "-S" $socket_path $ssh_target
                print $"  ✅ Closed connection to ($ssh_target)"
                $closed_count = $closed_count + 1
            } catch {
                print $"  ⚠️  Failed to close ($socket_name)"
            }
            
            # Supprimer le fichier socket
            try {
                rm $socket_path
            } catch {
                print $"  ⚠️  Could not remove socket file"
            }
        }
    }
    
    print $"✅ Closed ($closed_count) master connections"
}

# Fonction pour fermer la connexion de l'hôte actuellement sélectionné
export def close_current_master_connection [] {
    let context = load_context
    let current_host_info = $context.host | values | first
    
    if ($current_host_info.hostname == "localhost") {
        print "ℹ️  No master connection to close for localhost"
        return
    }
    
    close_master_connection $current_host_info
}

# Fonction pour lister toutes les connexions maîtres actives
export def list_master_connections [] {
    let control_dir = get_control_path
    
    print "🔍 Active master connections:"
    print "=" * 50
    
    let sockets = try {
        ls $control_dir | where type == file | get name
    } catch {
        []
    }
    
    if ($sockets | is-empty) {
        print "ℹ️  No master connections found"
        return
    }
    
    for socket_path in $sockets {
        let socket_name = ($socket_path | path basename)
        
        # Extraire user@hostname depuis le nom du socket (format: user@hostname:port)
        let parts = ($socket_name | split row ":")
        let ssh_target = if ($parts | length) > 0 { $parts | first } else { $socket_name }
        
        # Test si la connexion est vraiment active
        let is_active = try {
            run-external "ssh" "-O" "check" "-S" $socket_path $ssh_target
            "🟢 ACTIVE"
        } catch {
            "🔴 INACTIVE"
        }
        
        print $"  ($socket_name) - ($is_active)"
    }
}


export alias "ppo close" = close_current_master_connection
export alias "ppo closeall" = close_all_master_connections  
export alias "ppo lsconn" = list_master_connections