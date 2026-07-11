###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use machine-manager/ *
use context/ *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Internal helper functions                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Function to get the path of the control sockets directory
def get_control_path [] {
    let control_dir = "~/dev/nu-modules/PurposeOps/controlmasters" | path expand

    # Create directory if it doesn't exist
    if not ($control_dir | path exists) {
        mkdir $control_dir
    }

    $control_dir
}

# Function to generate the control socket name
def get_control_socket [host_info: record] {
    let control_dir = get_control_path
    let socket_name = $"($host_info.user)@($host_info.hostname):($host_info.port)"
    $"($control_dir)/($socket_name)"
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################
export def resolve_key_path [identity_file: string] {
    if ($identity_file | str starts-with "~/") {
        $identity_file | str replace "~" $env.HOME
    } else if ($identity_file | str starts-with "./") {
        $identity_file | path expand
    } else {
        $identity_file
    }
}

# Function to check if a master connection exists and is active
export def is_master_active [host_info: record] {
    let socket_path = get_control_socket $host_info

    # Check if socket exists
    if not ($socket_path | path exists) {
        return false
    }

    # Test if connection is really active
    let ssh_target = $"($host_info.user)@($host_info.hostname)"
    let test_result = try {
        run-external "ssh" "-O" "check" "-S" $socket_path $ssh_target
        true
    } catch {
        false
    }

    $test_result
}

# Function to create a master connection
export def create_master_connection [host_info: record] {
    let socket_path = get_control_socket $host_info
    let ssh_target = $"($host_info.user)@($host_info.hostname)"
    print $"🔄 Creating master connection to ($ssh_target)..."

    if ($socket_path | path exists) and (not (is_master_active $host_info)) {
        print $"🧹 Nettoyage d'un socket orphelin..."
        rm $socket_path
    }

    let ssh_args = [
        "-M" "-N" "-f" "-n"
        "-S" $socket_path
        "-p" ($host_info.port | into string)
        "-o" "StrictHostKeyChecking=no"
        "-o" "ConnectTimeout=10"
    ]

    let ssh_args = if (($host_info.identity_file | is-not-empty) and ($host_info.identity_file != "")) {
        let key_path = (resolve_key_path $host_info.identity_file)
        ["-i", $key_path] | append $ssh_args
    } else {
        $ssh_args
    }

    # FIX : on capture explicitement l'échec ici, car -f peut retourner un code non-nul
    # de façon bénigne. On ne veut jamais qu'une erreur ici fasse planter tout le reste ;
    # le vrai verdict se fait juste après via is_master_active.
    try {
        run-external "ssh" ...$ssh_args $ssh_target
    } catch { |e|
        print $"⚠️ ssh -M a retourné une erreur (potentiellement bénigne) : ($e.msg)"
    }

    sleep 500ms

    if (is_master_active $host_info) {
        print $"✅ Master connection established."
        return true
    } else {
        print $"❌ Échec création master : le socket existe mais est inactif."
        if ($socket_path | path exists) { rm $socket_path }
        return false
    }
}
# Function to execute a command via the master connection
export def run_with_master [host_info: record, command: string] {
    let socket_path = get_control_socket $host_info
    let ssh_target = $"($host_info.user)@($host_info.hostname)"

    # Ensure we have an active master connection
    if not (is_master_active $host_info) {
        if not (create_master_connection $host_info) {
            error make {msg: "Failed to establish master connection"}
        }
    }

    # FIX: Escape braces for SSH
    let escaped_command = $command | str replace --all "{{" "\\{\\{" | str replace --all "}}" "\\}\\}"

    # Build SSH arguments
    let ssh_args = [
        "-S" $socket_path
        "-p" $host_info.port
        "-o" "StrictHostKeyChecking=no"
        "-o" "ConnectTimeout=10"
    ]

    # Add private key if needed
    let ssh_args = if ($host_info.identity_file != "") {
        let key_path = resolve_key_path $host_info.identity_file
        $ssh_args | append ["-i" $key_path]
    } else {
        $ssh_args
    }

    # Execute command via master connection
    run-external "ssh" ...$ssh_args $ssh_target $escaped_command
}

# Function to close a specific master connection
export def close_master_connection [host_info: record] {
    let socket_path = get_control_socket $host_info
    let ssh_target = $"($host_info.user)@($host_info.hostname)"

    # Check if connection exists
    if not ($socket_path | path exists) {
        print $"ℹ️  No master connection exists for ($ssh_target)"
        return true
    }

    # Check if connection is active
    if not (is_master_active $host_info) {
        print $"ℹ️  Master connection for ($ssh_target) is already inactive"
        # Clean up orphaned socket
        rm $socket_path
        return true
    }

    print $"🔄 Closing master connection to ($ssh_target)..."

    # Close master connection
    let result = try {
        run-external "ssh" "-O" "exit" "-S" $socket_path $ssh_target
        print $"✅ Master connection closed for ($ssh_target)"
        true
    } catch { |err|
        print $"❌ Failed to close master connection: ($err.msg)"
        false
    }

    # Clean up socket if it still exists
    if ($socket_path | path exists) {
        rm $socket_path
    }

    $result
}

# Function to close all active master connections
export def close_all_master_connections [] {
    let control_dir = get_control_path

    print "🔄 Closing all master connections..."

    # Search for all sockets in control directory
    let sockets = try {
        ls $control_dir | where type == socket | get name
    } catch {
        []
    }

    if ($sockets | is-empty) {
        print "ℹ️  No master connections found"
        return
    }

    mut closed_count = 0

    # Close each connection
    for socket_path in $sockets {
        let socket_name = ($socket_path | path basename)
        print $"🔄 Processing ($socket_name)..."

        # Extract connection info from socket name
        # Format: user@hostname:port
        let parts = ($socket_name | parse "{user}@{hostname}:{port}")

        if ($parts | length) > 0 {
            let conn_info = $parts | first
            let host_info = {
                user: $conn_info.user
                hostname: $conn_info.hostname  
                port: ($conn_info.port | into int)
                identity_file: ""  # We don't have this info from socket
            }

            try {
                let ssh_target = $"($host_info.user)@($host_info.hostname)"
                run-external "ssh" "-O" "exit" "-S" $socket_path $ssh_target
                print $"  ✅ Closed connection to ($ssh_target)"
                $closed_count = $closed_count + 1
            } catch {
                print $"  ⚠️  Failed to close ($socket_name)"
            }

            # Remove socket file
            try {
                rm $socket_path
            } catch {
                print $"  ⚠️  Could not remove socket file"
            }
        }
    }

    print $"✅ Closed ($closed_count) master connections"
}

# Function to close the connection of the currently selected host
export def close_current_master_connection [] {
    let context = load_context
    let current_host_info = $context.host | values | first

    if ($current_host_info.hostname == "localhost") {
        print "ℹ️  No master connection to close for localhost"
        return
    }

    close_master_connection $current_host_info
}

# Function to list all active master connections
export def list_master_connections [] {
    let control_dir = get_control_path

    print "🔍 Active master connections:"

    let sockets = try {
        ls $control_dir | where type == socket | get name
    } catch {
        []
    }

    if ($sockets | is-empty) {
        print "ℹ️  No master connections found"
        return
    }

    for socket_path in $sockets {
        let socket_name = ($socket_path | path basename)

        # Parse socket name to extract connection info (format: user@hostname:port)
        let parts = ($socket_name | parse "{user}@{hostname}:{port}")
        
        if ($parts | length) > 0 {
            let conn_info = $parts | first
            let host_info = {
                user: $conn_info.user
                hostname: $conn_info.hostname
                port: ($conn_info.port | into int)
                identity_file: ""  # Not needed for status check
            }
            
            # Use existing function to check if master is active
            let is_active = if (is_master_active $host_info) {
                "🟢 ACTIVE"
            } else {
                "🔴 INACTIVE"
            }

            print $"  ($socket_name) - ($is_active)"
        } else {
            # If parsing fails, still show the socket but mark as unknown format
            print $"  ($socket_name) - ❓ UNKNOWN FORMAT"
        }
    }
}
###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "close" = close_current_master_connection
export alias "closeall" = close_all_master_connections  
export alias "lsconn" = list_master_connections